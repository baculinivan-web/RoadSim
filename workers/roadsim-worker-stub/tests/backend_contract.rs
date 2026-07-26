//! Contract test: the SUMO runner backend drives a protocol worker end to end.
//!
//! The stub speaks worker protocol v3 but is not SUMO, so this suite proves
//! the backend side — export materialization, run directories, lifecycle,
//! pause/resume, cancellation and terminal outcomes — without an engine.
//! Frames stay empty because the stub publishes none; the real-engine frame
//! path is covered by the opt-in SUMO smokes.

use roadsim_backend_api::{
    BackendErrorCode, BackendEvent, ClientHello, CompileOptions, ControlCommand, ControlKind,
    RunConfig, ScenarioSnapshot, SeedAlgorithmVersion, SessionState, SimulationBackend,
};
use roadsim_backend_sumo::{SumoRoadExportOptions, SumoVehicleTypeOptions};
use roadsim_backend_sumo_client::{
    NetworkMaterialization, SUMO_BACKEND_ID, SumoRunnerBackend, SumoRunnerConfig,
};
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledLaneUse, CompiledNetwork, CompiledNetworkHeader,
    CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_types::{CorridorId, LaneId, RootSeed, Sha256Digest, SimulationTick};
use roadsim_worker_protocol::AuthToken;
use std::{sync::Arc, time::Duration};

const STUB_CAPABILITY: &str = "worker.stub.lifecycle";

fn network() -> Arc<CompiledNetwork> {
    let lanes = LaneTable::new(
        vec![CompiledPoint::new(0.0, 0.0)],
        vec![CompiledPoint::new(100.0, 0.0)],
        vec![3.5],
        vec![CompiledLaneUse::GeneralTraffic],
    )
    .unwrap();
    Arc::new(
        CompiledNetwork::new(
            CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([5; 32])),
            lanes,
            vec![LaneOrigin::new(
                CorridorId::from_u128(10),
                LaneId::from_u128(11),
            )],
            CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
        )
        .unwrap(),
    )
}

fn backend(root: &std::path::Path) -> SumoRunnerBackend {
    SumoRunnerBackend::new(
        SumoRunnerConfig::new(
            env!("CARGO_BIN_EXE_roadsim-worker-stub"),
            NetworkMaterialization::PlainFilesOnly,
            root,
            AuthToken::parse("b".repeat(64)).unwrap(),
            SumoRoadExportOptions::new(13.89).unwrap(),
            SumoVehicleTypeOptions::new(4.5, 1.8, 13.89).unwrap(),
        )
        .with_required_capabilities(vec![STUB_CAPABILITY.to_owned()])
        .with_request_timeout(Duration::from_secs(5)),
    )
}

fn run_config() -> RunConfig {
    RunConfig::new(
        Sha256Digest::from_bytes([6; 32]),
        8,
        2,
        RootSeed::new(7),
        SeedAlgorithmVersion::V1,
    )
    .unwrap()
}

fn scenario() -> ScenarioSnapshot {
    ScenarioSnapshot::new(Sha256Digest::from_bytes([6; 32]), 1).unwrap()
}

fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "roadsim-backend-contract-{tag}-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn the_backend_runs_a_protocol_worker_to_completion() {
    pollster::block_on(async {
        let root = temp_root("complete");
        let backend = backend(&root);
        backend.handshake(ClientHello::current()).await.unwrap();
        let artifact = backend
            .compile(network(), scenario(), CompileOptions::none())
            .await
            .unwrap();
        assert_eq!(artifact.backend_id(), SUMO_BACKEND_ID);

        // The export bundle was materialized into the bounded run directory.
        let run_dir = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.is_dir())
            .unwrap();
        for name in [
            "roadsim.nod.xml",
            "roadsim.edg.xml",
            "roadsim.con.xml",
            "roadsim.tll.xml",
            "roadsim.sumocfg",
        ] {
            assert!(run_dir.join(name).is_file(), "{name} must be materialized");
        }

        let mut session = backend.start(artifact, run_config()).await.unwrap();
        assert_eq!(session.state(), SessionState::Running);

        let mut ticks = Vec::new();
        let summary = loop {
            match session.next_event().await.unwrap() {
                BackendEvent::StateChanged(state) => assert_eq!(state, SessionState::Running),
                BackendEvent::Frame(frame) => ticks.push(frame.tick().get()),
                BackendEvent::Completed(summary) => break summary,
            }
        };
        // step_ticks = 2 over 8 duration ticks: monotone, exact coverage.
        assert_eq!(ticks, vec![2, 4, 6, 8]);
        assert_eq!(summary.terminal_state(), SessionState::Completed);
        assert_eq!(summary.final_tick(), SimulationTick::new(8));
        assert_eq!(summary.emitted_frames(), 4);
        assert_eq!(session.state(), SessionState::Completed);
        std::fs::remove_dir_all(root).unwrap();
    });
}

#[test]
fn pause_stops_stepping_and_resume_continues_the_same_run() {
    pollster::block_on(async {
        let root = temp_root("pause");
        let backend = backend(&root);
        backend.handshake(ClientHello::current()).await.unwrap();
        let artifact = backend
            .compile(network(), scenario(), CompileOptions::none())
            .await
            .unwrap();
        let mut session = backend.start(artifact, run_config()).await.unwrap();

        // Announce + one stepped frame.
        session.next_event().await.unwrap();
        let BackendEvent::Frame(first) = session.next_event().await.unwrap() else {
            panic!("expected a frame");
        };
        assert_eq!(first.tick(), SimulationTick::new(2));

        session
            .control(ControlCommand::new(1, 1, ControlKind::Pause))
            .await
            .unwrap();
        assert_eq!(session.state(), SessionState::Paused);
        // A paused session reports its state and must not advance the tick.
        let BackendEvent::StateChanged(state) = session.next_event().await.unwrap() else {
            panic!("expected a state event");
        };
        assert_eq!(state, SessionState::Paused);

        // Pause twice is an invalid lifecycle transition, not a panic.
        let error = session
            .control(ControlCommand::new(2, 2, ControlKind::Pause))
            .await
            .unwrap_err();
        assert_eq!(error.code(), BackendErrorCode::InvalidLifecycle);

        session
            .control(ControlCommand::new(3, 3, ControlKind::Resume))
            .await
            .unwrap();
        let BackendEvent::Frame(second) = session.next_event().await.unwrap() else {
            panic!("expected a frame after resume");
        };
        assert_eq!(second.tick(), SimulationTick::new(4));

        let summary = session.cancel().await.unwrap();
        assert_eq!(summary.terminal_state(), SessionState::Cancelled);
        assert_eq!(summary.final_tick(), SimulationTick::new(4));

        // A terminal session refuses further control commands.
        let error = session
            .control(ControlCommand::new(4, 4, ControlKind::Resume))
            .await
            .unwrap_err();
        assert_eq!(error.code(), BackendErrorCode::TerminalSession);
        std::fs::remove_dir_all(root).unwrap();
    });
}

#[test]
fn a_foreign_artifact_is_rejected_before_any_worker_is_spawned() {
    pollster::block_on(async {
        let root = temp_root("foreign");
        let backend = backend(&root);
        let foreign = roadsim_backend_api::BackendArtifact::new(
            roadsim_backend_api::BackendId::new("roadsim.fake.v1"),
            Sha256Digest::from_bytes([1; 32]),
            Sha256Digest::from_bytes([2; 32]),
        );
        let Err(error) = backend.start(foreign, run_config()).await else {
            panic!("foreign artifact must be rejected");
        };
        assert_eq!(error.code(), BackendErrorCode::ArtifactBackendMismatch);

        let unknown = roadsim_backend_api::BackendArtifact::new(
            SUMO_BACKEND_ID,
            Sha256Digest::from_bytes([1; 32]),
            Sha256Digest::from_bytes([2; 32]),
        );
        let Err(error) = backend.start(unknown, run_config()).await else {
            panic!("unknown artifact must be rejected");
        };
        assert_eq!(error.code(), BackendErrorCode::ArtifactNotFound);
        std::fs::remove_dir_all(root).unwrap();
    });
}
