use proptest::prelude::*;
use roadsim_backend_api::{
    BackendErrorCode, BackendErrorPhase, BackendEvent, ClientHello, CompileOptions, ControlCommand,
    ControlKind, RunConfig, ScenarioSnapshot, SeedAlgorithmVersion, SessionState,
    SimulationBackend,
};
use roadsim_backend_fake::FakeBackend;
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledLaneUse, CompiledNetwork, CompiledNetworkHeader,
    CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_types::{CorridorId, LaneId, RootSeed, Sha256Digest};
use std::sync::Arc;

fn network_with(capability: CapabilityId) -> Arc<CompiledNetwork> {
    let lanes = LaneTable::new(
        vec![CompiledPoint::new(0.0, 0.0)],
        vec![CompiledPoint::new(100.0, 0.0)],
        vec![3.5],
        vec![CompiledLaneUse::GeneralTraffic],
    )
    .unwrap();
    Arc::new(
        CompiledNetwork::new(
            CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([1; 32])),
            lanes,
            vec![LaneOrigin::new(
                CorridorId::from_u128(1),
                LaneId::from_u128(2),
            )],
            CapabilityRequirements::new([capability]),
        )
        .unwrap(),
    )
}

fn scenario(agent_count: u32) -> ScenarioSnapshot {
    ScenarioSnapshot::new(Sha256Digest::from_bytes([2; 32]), agent_count).unwrap()
}

fn config(seed: u64, duration: u64, step: u64) -> RunConfig {
    RunConfig::new(
        Sha256Digest::from_bytes([2; 32]),
        duration,
        step,
        RootSeed::new(seed),
        SeedAlgorithmVersion::V1,
    )
    .unwrap()
}

#[test]
fn handshake_and_capability_preflight_are_explicit() {
    pollster::block_on(async {
        let backend = FakeBackend::new();
        let hello = backend.handshake(ClientHello::current()).await.unwrap();
        assert!(hello.deterministic());
        assert_eq!(hello.api_version(), 1);
        assert!(
            hello
                .capabilities()
                .contains(&CapabilityId::RoadVehiclesBasic)
        );

        let mismatch = backend
            .handshake(ClientHello::with_version(99))
            .await
            .unwrap_err();
        assert_eq!(mismatch.phase(), BackendErrorPhase::Handshake);
        assert_eq!(mismatch.code(), BackendErrorCode::ProtocolMismatch);

        let unsupported = backend
            .compile(
                network_with(CapabilityId::TransitBusLanes),
                scenario(1),
                CompileOptions,
            )
            .await
            .unwrap_err();
        assert_eq!(unsupported.phase(), BackendErrorPhase::Unsupported);
        assert_eq!(
            unsupported.unsupported_capabilities(),
            &[CapabilityId::TransitBusLanes]
        );
    });
}

#[test]
fn session_pause_resume_idempotency_and_cancel_follow_lifecycle() {
    pollster::block_on(async {
        let backend = FakeBackend::new();
        let artifact = backend
            .compile(
                network_with(CapabilityId::RoadVehiclesBasic),
                scenario(3),
                CompileOptions,
            )
            .await
            .unwrap();
        let mut session = backend.start(artifact, config(7, 100, 10)).await.unwrap();

        assert_eq!(
            session.next_event().await.unwrap(),
            BackendEvent::StateChanged(SessionState::Running)
        );
        let BackendEvent::Frame(first) = session.next_event().await.unwrap() else {
            panic!("expected first frame");
        };
        assert_eq!(first.tick().get(), 0);
        assert_eq!(first.agents().len(), 3);

        let pause = ControlCommand::new(1, 100, ControlKind::Pause);
        assert!(!session.control(pause).await.unwrap().duplicate());
        assert!(session.control(pause).await.unwrap().duplicate());
        assert_eq!(
            session.next_event().await.unwrap(),
            BackendEvent::StateChanged(SessionState::Paused)
        );
        assert_eq!(
            session.next_event().await.unwrap_err().code(),
            BackendErrorCode::InvalidLifecycle
        );

        session
            .control(ControlCommand::new(2, 101, ControlKind::Resume))
            .await
            .unwrap();
        assert_eq!(
            session.next_event().await.unwrap(),
            BackendEvent::StateChanged(SessionState::Running)
        );
        let summary = session.cancel().await.unwrap();
        assert_eq!(summary.terminal_state(), SessionState::Cancelled);
        assert_eq!(summary.emitted_frames(), 1);
        assert_eq!(
            session.next_event().await.unwrap_err().code(),
            BackendErrorCode::TerminalSession
        );
    });
}

#[test]
fn same_inputs_emit_identical_frames_and_complete_once() {
    pollster::block_on(async {
        let backend = FakeBackend::new();
        let artifact = backend
            .compile(
                network_with(CapabilityId::RoadVehiclesBasic),
                scenario(4),
                CompileOptions,
            )
            .await
            .unwrap();
        let mut first = backend.start(artifact, config(42, 3, 1)).await.unwrap();
        let mut second = backend.start(artifact, config(42, 3, 1)).await.unwrap();

        assert_eq!(
            first.next_event().await.unwrap(),
            second.next_event().await.unwrap()
        );
        for tick in 0..3 {
            let first_event = first.next_event().await.unwrap();
            let second_event = second.next_event().await.unwrap();
            assert_eq!(first_event, second_event);
            let BackendEvent::Frame(frame) = first_event else {
                panic!("expected frame");
            };
            assert_eq!(frame.tick().get(), tick);
        }
        let BackendEvent::Completed(summary) = first.next_event().await.unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(summary.terminal_state(), SessionState::Completed);
        assert_eq!(summary.final_tick().get(), 3);
        assert_eq!(summary.emitted_frames(), 3);
        assert_eq!(
            first.next_event().await.unwrap_err().code(),
            BackendErrorCode::TerminalSession
        );
    });
}

proptest! {
    #[test]
    fn generated_agent_frames_are_finite_and_on_the_compiled_lane(
        seed in any::<u64>(),
        agent_count in 1_u32..100,
        tick in 1_u64..1_000,
    ) {
        pollster::block_on(async {
            let backend = FakeBackend::new();
            let artifact = backend
                .compile(
                    network_with(CapabilityId::RoadVehiclesBasic),
                    scenario(agent_count),
                    CompileOptions,
                )
                .await
                .unwrap();
            let mut session = backend.start(artifact, config(seed, tick + 1, tick)).await.unwrap();
            let _ = session.next_event().await.unwrap();
            let BackendEvent::Frame(frame) = session.next_event().await.unwrap() else {
                panic!("expected frame");
            };
            for agent in frame.agents() {
                prop_assert!(agent.x_m().is_finite());
                prop_assert!(agent.y_m().is_finite());
                prop_assert!(agent.heading_rad().is_finite());
                prop_assert!((0.0..=100.0).contains(&agent.x_m()));
                prop_assert_eq!(agent.y_m(), 0.0);
            }
            Ok(())
        })?;
    }
}
