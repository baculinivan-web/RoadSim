use roadsim_worker_client::{
    RunDirectoryLimits, RunDirectoryManager, RunState, WorkerClient, WorkerClientConfig,
    WorkerClientErrorCode,
};
use roadsim_worker_protocol::{
    AuthToken, EngineIdentity, WorkerDiagnosticCode, WorkerSessionConfig,
};
use std::{path::PathBuf, process::Command, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(2);
const REAL_LIBSUMO_TIMEOUT: Duration = Duration::from_secs(10);

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
            vec!["simulation.lifecycle.step.v1".to_owned()],
            exact_engine(),
            TIMEOUT,
        )
        .unwrap();
    let config = WorkerSessionConfig::new("bundle/run.sumocfg", 7, 100).unwrap();
    client.open_session(1, config, TIMEOUT).unwrap();
    assert_eq!(client.step_session(1, 3, TIMEOUT).unwrap(), 3);
    assert_eq!(client.step_session(1, 2, TIMEOUT).unwrap(), 5);
    client.close_session(1, TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();

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
            vec!["simulation.lifecycle.step.v1".to_owned()],
            exact_engine(),
            REAL_LIBSUMO_TIMEOUT,
        )
        .unwrap();
    assert_eq!(hello.engine, exact_engine());
    let config = WorkerSessionConfig::new("minimal.sumocfg", 7, 100).unwrap();
    client
        .open_session(91, config, REAL_LIBSUMO_TIMEOUT)
        .unwrap();
    assert_eq!(client.step_session(91, 5, REAL_LIBSUMO_TIMEOUT).unwrap(), 5);
    client.close_session(91, REAL_LIBSUMO_TIMEOUT).unwrap();
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
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge_v1.c");
    let mut command = Command::new("cc");
    if cfg!(target_os = "macos") {
        command.arg("-dynamiclib");
    } else {
        command.args(["-shared", "-fPIC"]);
    }
    let status = command.arg(source).arg("-o").arg(output).status().unwrap();
    assert!(status.success());
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
