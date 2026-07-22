use roadsim_backend_sumo::{
    SUMO_EDGES_FILE, SUMO_NODES_FILE, SumoRoadExportOptions, export_straight_network,
};
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledLaneUse, CompiledNetwork, CompiledNetworkHeader,
    CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_types::{CorridorId, LaneId, Sha256Digest};
use roadsim_worker_client::{
    RunDirectoryLimits, RunDirectoryManager, RunState, WorkerClient, WorkerClientConfig,
    WorkerClientErrorCode,
};
use roadsim_worker_protocol::{
    AuthToken, EngineIdentity, VisualFrameBatch, WorkerDiagnosticCode, WorkerSessionConfig,
};
use std::{
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(2);
const REAL_LIBSUMO_TIMEOUT: Duration = Duration::from_secs(10);
const LIFECYCLE_CAPABILITY: &str = "simulation.lifecycle.step.v1";
const VEHICLE_VISUAL_CAPABILITY: &str = "simulation.visual.vehicle.v1";

fn token() -> AuthToken {
    AuthToken::parse("a".repeat(64)).unwrap()
}

fn exact_engine() -> EngineIdentity {
    EngineIdentity::new(
        "eclipse.sumo",
        "1.27.1",
        "7717f2379d9e314a0c81c5cec748444de06a2a91",
    )
    .unwrap()
}

#[test]
fn missing_native_bridge_is_rejected_before_session_open() {
    let mut client = WorkerClient::spawn(
        WorkerClientConfig::new(env!("CARGO_BIN_EXE_sumo-worker"), token())
            .with_argument("--bridge")
            .with_argument("definitely-missing-roadsim-sumo-bridge"),
    )
    .unwrap();
    let required = exact_engine();
    let error = client
        .handshake_with_engine(
            vec!["simulation.lifecycle.step.v1".to_owned()],
            required,
            TIMEOUT,
        )
        .unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerRejected);
    assert_eq!(
        error.worker_diagnostic(),
        Some(WorkerDiagnosticCode::EngineUnavailable)
    );
}

#[cfg(unix)]
#[test]
fn bridge_abi_runs_start_step_close_and_isolates_native_crash() {
    let root = std::env::temp_dir().join(format!(
        "roadsim-sumo-bridge-fixture-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir(&root).unwrap();
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let bridge = root.join(format!("libroadsim_sumo_bridge_fixture.{extension}"));
    compile_fixture(&bridge);

    let mut client = spawn_with_bridge(&bridge, "b");
    client
        .handshake_with_engine(
            vec![
                LIFECYCLE_CAPABILITY.to_owned(),
                VEHICLE_VISUAL_CAPABILITY.to_owned(),
            ],
            exact_engine(),
            TIMEOUT,
        )
        .unwrap();
    let config = WorkerSessionConfig::new("bundle/run.sumocfg", 7, 100).unwrap();
    client.open_session(1, config, TIMEOUT).unwrap();
    assert_eq!(client.step_session(1, 3, TIMEOUT).unwrap(), 3);
    let (session_id, _, frame) = wait_for_visual(&client, TIMEOUT);
    assert_eq!(session_id, 1);
    assert_eq!(frame.tick(), 3);
    assert_eq!(frame.agent_ids(), &[7, 42]);
    assert_eq!(frame.length_m(), &[4.5, 12.0]);
    assert_eq!(frame.width_m(), &[1.8, 2.5]);
    assert_eq!(client.step_session(1, 2, TIMEOUT).unwrap(), 5);
    let (_, _, frame) = wait_for_visual(&client, TIMEOUT);
    assert_eq!(frame.tick(), 5);
    assert_eq!(frame.x_m(), &[5.0, 15.0]);
    client.close_session(1, TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();

    let mut invalid = spawn_with_bridge(&bridge, "d");
    invalid
        .handshake_with_engine(
            vec![VEHICLE_VISUAL_CAPABILITY.to_owned()],
            exact_engine(),
            TIMEOUT,
        )
        .unwrap();
    let config = WorkerSessionConfig::new("bundle/invalid-frame.sumocfg", 7, 100).unwrap();
    invalid.open_session(3, config, TIMEOUT).unwrap();
    let error = invalid.step_session(3, 1, TIMEOUT).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerRejected);
    assert_eq!(
        error.worker_diagnostic(),
        Some(WorkerDiagnosticCode::EngineStepFailed)
    );
    invalid.close_session(3, TIMEOUT).unwrap();
    invalid.shutdown(TIMEOUT).unwrap();

    let mut crashing = spawn_with_bridge(&bridge, "c");
    crashing
        .handshake_with_engine(Vec::new(), exact_engine(), TIMEOUT)
        .unwrap();
    let config = WorkerSessionConfig::new("bundle/crash.sumocfg", 7, 100).unwrap();
    crashing.open_session(2, config, TIMEOUT).unwrap();
    let error = crashing.step_session(2, 1, TIMEOUT).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerExited);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires ROADSIM_SUMO_BRIDGE and ROADSIM_NETCONVERT for exact SUMO 1.27.1"]
fn real_libsumo_runs_minimal_start_step_close() {
    let bridge = PathBuf::from(
        std::env::var_os("ROADSIM_SUMO_BRIDGE").expect("ROADSIM_SUMO_BRIDGE is required"),
    );
    let netconvert = PathBuf::from(
        std::env::var_os("ROADSIM_NETCONVERT").expect("ROADSIM_NETCONVERT is required"),
    );
    assert!(bridge.is_absolute(), "ROADSIM_SUMO_BRIDGE must be absolute");
    assert!(
        netconvert.is_absolute(),
        "ROADSIM_NETCONVERT must be absolute"
    );
    let root =
        std::env::temp_dir().join(format!("roadsim-real-libsumo-smoke-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manager = RunDirectoryManager::new(&root, RunDirectoryLimits::new(1, 8).unwrap()).unwrap();
    let mut run = manager.create_run(91, 1).unwrap();
    copy_minimal_fixture(run.path());
    let output = Command::new(netconvert)
        .current_dir(run.path())
        .args([
            "--node-files",
            "minimal.nod.xml",
            "--edge-files",
            "minimal.edg.xml",
            "--output-file",
            "minimal.net.xml",
            "--no-turnarounds",
            "true",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "netconvert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    run.mark_running().unwrap();

    let mut client = WorkerClient::spawn(
        WorkerClientConfig::new(env!("CARGO_BIN_EXE_sumo-worker"), token())
            .with_argument("--bridge")
            .with_argument(bridge)
            .with_work_directory(run.path()),
    )
    .unwrap();
    let hello = client
        .handshake_with_engine(
            vec![
                LIFECYCLE_CAPABILITY.to_owned(),
                VEHICLE_VISUAL_CAPABILITY.to_owned(),
            ],
            exact_engine(),
            REAL_LIBSUMO_TIMEOUT,
        )
        .unwrap();
    assert_eq!(hello.engine, exact_engine());
    assert!(
        hello
            .capabilities
            .contains(&VEHICLE_VISUAL_CAPABILITY.to_owned())
    );
    let config = WorkerSessionConfig::new("minimal.sumocfg", 7, 100).unwrap();
    client
        .open_session(91, config, REAL_LIBSUMO_TIMEOUT)
        .unwrap();
    assert_eq!(client.step_session(91, 5, REAL_LIBSUMO_TIMEOUT).unwrap(), 5);
    let (session_id, _, frame) = wait_for_visual(&client, REAL_LIBSUMO_TIMEOUT);
    assert_eq!(session_id, 91);
    assert_eq!(frame.tick(), 5);
    assert_eq!(frame.agent_ids(), &[0]);
    assert_eq!(frame.length_m(), &[4.5]);
    assert_eq!(frame.width_m(), &[1.8]);
    assert!((2.0..4.0).contains(&frame.x_m()[0]));
    assert!(frame.heading_rad()[0].abs() < 1.0e-9);
    client.close_session(91, REAL_LIBSUMO_TIMEOUT).unwrap();
    client.shutdown(REAL_LIBSUMO_TIMEOUT).unwrap();
    run.mark_completed().unwrap();
    drop(run);
    assert_eq!(manager.recover().unwrap()[0].state(), RunState::Completed);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires ROADSIM_SUMO_BRIDGE and ROADSIM_NETCONVERT for exact SUMO 1.27.1"]
fn real_libsumo_runs_exported_straight_csn() {
    let bridge = PathBuf::from(
        std::env::var_os("ROADSIM_SUMO_BRIDGE").expect("ROADSIM_SUMO_BRIDGE is required"),
    );
    let netconvert = PathBuf::from(
        std::env::var_os("ROADSIM_NETCONVERT").expect("ROADSIM_NETCONVERT is required"),
    );
    assert!(bridge.is_absolute(), "ROADSIM_SUMO_BRIDGE must be absolute");
    assert!(
        netconvert.is_absolute(),
        "ROADSIM_NETCONVERT must be absolute"
    );
    let root = std::env::temp_dir().join(format!(
        "roadsim-real-exported-csn-smoke-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manager = RunDirectoryManager::new(&root, RunDirectoryLimits::new(1, 8).unwrap()).unwrap();
    let mut run = manager.create_run(92, 1).unwrap();
    let network = one_lane_network();
    let bundle =
        export_straight_network(&network, SumoRoadExportOptions::new(13.89).unwrap()).unwrap();
    assert_eq!(
        bundle.lane_mappings()[0].origin(),
        network
            .lane_origin(bundle.lane_mappings()[0].compiled_lane_id())
            .unwrap()
    );
    std::fs::write(run.path().join(SUMO_NODES_FILE), bundle.nodes_xml()).unwrap();
    std::fs::write(run.path().join(SUMO_EDGES_FILE), bundle.edges_xml()).unwrap();
    std::fs::write(
        run.path().join("roadsim.sumocfg"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<configuration>\n    <input><net-file value=\"roadsim.net.xml\"/></input>\n    <time><begin value=\"0\"/><end value=\"1\"/></time>\n</configuration>\n",
    )
    .unwrap();
    let output = Command::new(netconvert)
        .current_dir(run.path())
        .args([
            "--node-files",
            SUMO_NODES_FILE,
            "--edge-files",
            SUMO_EDGES_FILE,
            "--output-file",
            "roadsim.net.xml",
            "--no-turnarounds",
            "true",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "netconvert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    run.mark_running().unwrap();

    let mut client = WorkerClient::spawn(
        WorkerClientConfig::new(env!("CARGO_BIN_EXE_sumo-worker"), token())
            .with_argument("--bridge")
            .with_argument(bridge)
            .with_work_directory(run.path()),
    )
    .unwrap();
    client
        .handshake_with_engine(
            vec![
                LIFECYCLE_CAPABILITY.to_owned(),
                VEHICLE_VISUAL_CAPABILITY.to_owned(),
            ],
            exact_engine(),
            REAL_LIBSUMO_TIMEOUT,
        )
        .unwrap();
    let config = WorkerSessionConfig::new("roadsim.sumocfg", 7, 100).unwrap();
    client
        .open_session(92, config, REAL_LIBSUMO_TIMEOUT)
        .unwrap();
    assert_eq!(client.step_session(92, 5, REAL_LIBSUMO_TIMEOUT).unwrap(), 5);
    let (session_id, _, frame) = wait_for_visual(&client, REAL_LIBSUMO_TIMEOUT);
    assert_eq!(session_id, 92);
    assert_eq!(frame.tick(), 5);
    assert!(frame.is_empty());
    client.close_session(92, REAL_LIBSUMO_TIMEOUT).unwrap();
    client.shutdown(REAL_LIBSUMO_TIMEOUT).unwrap();
    run.mark_completed().unwrap();
    drop(run);
    assert_eq!(manager.recover().unwrap()[0].state(), RunState::Completed);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn spawn_with_bridge(bridge: &PathBuf, token_character: &str) -> WorkerClient {
    WorkerClient::spawn(
        WorkerClientConfig::new(
            env!("CARGO_BIN_EXE_sumo-worker"),
            AuthToken::parse(token_character.repeat(64)).unwrap(),
        )
        .with_argument("--bridge")
        .with_argument(bridge),
    )
    .unwrap()
}

#[cfg(unix)]
fn compile_fixture(output: &PathBuf) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge_v2.c");
    let mut command = Command::new("cc");
    if cfg!(target_os = "macos") {
        command.arg("-dynamiclib");
    } else {
        command.args(["-shared", "-fPIC"]);
    }
    let status = command.arg(source).arg("-o").arg(output).status().unwrap();
    assert!(status.success());
}

fn wait_for_visual(client: &WorkerClient, timeout: Duration) -> (u64, u64, VisualFrameBatch) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(frame) = client.take_latest_visual_frame() {
            return frame;
        }
        assert_eq!(client.data_stream_error(), None);
        assert!(Instant::now() < deadline, "visual frame timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

fn copy_minimal_fixture(destination: &std::path::Path) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sumo/minimal");
    for name in [
        "minimal.nod.xml",
        "minimal.edg.xml",
        "minimal.rou.xml",
        "minimal.sumocfg",
    ] {
        std::fs::copy(source.join(name), destination.join(name)).unwrap();
    }
}

fn one_lane_network() -> CompiledNetwork {
    let lanes = LaneTable::new(
        vec![CompiledPoint::new(0.0, 0.0)],
        vec![CompiledPoint::new(100.0, 0.0)],
        vec![3.5],
        vec![CompiledLaneUse::GeneralTraffic],
    )
    .unwrap();
    CompiledNetwork::new(
        CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([1; 32])),
        lanes,
        vec![LaneOrigin::new(
            CorridorId::from_u128(10),
            LaneId::from_u128(11),
        )],
        CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
    )
    .unwrap()
}
