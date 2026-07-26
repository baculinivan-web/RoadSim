use roadsim_application::{
    RunErrorCode, RunIntent, RunOrchestrator, RunOutcome, RunRequest, RunState,
};
use roadsim_backend_api::{
    ClientHello, CompileOptions, ControlCommand, ControlKind, RunConfig, ScenarioSnapshot,
    SeedAlgorithmVersion, SessionState, SimulationBackend,
};
use roadsim_backend_fake::FakeBackend;
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledLaneUse, CompiledNetwork, CompiledNetworkHeader,
    CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_types::{CorridorId, LaneId, RootSeed, Sha256Digest, SimulationTick};
use std::sync::Arc;

const EVERY_REQUEST: [RunRequest; 6] = [
    RunRequest::Prepare,
    RunRequest::Start,
    RunRequest::Pause,
    RunRequest::Resume,
    RunRequest::Cancel,
    RunRequest::Reset,
];

fn prepared() -> RunOrchestrator {
    let mut orchestrator = RunOrchestrator::new();
    orchestrator.request(RunRequest::Prepare).unwrap();
    orchestrator
}

fn running() -> RunOrchestrator {
    let mut orchestrator = prepared();
    orchestrator.request(RunRequest::Start).unwrap();
    orchestrator
}

#[test]
fn a_run_reaches_running_only_through_prepare_and_start() {
    let mut orchestrator = RunOrchestrator::new();
    assert_eq!(orchestrator.state(), RunState::Idle);
    assert_eq!(orchestrator.started_runs(), 0);

    assert_eq!(
        orchestrator.request(RunRequest::Prepare).unwrap(),
        RunIntent::Compile
    );
    assert_eq!(orchestrator.state(), RunState::Prepared);

    assert_eq!(
        orchestrator.request(RunRequest::Start).unwrap(),
        RunIntent::StartSession
    );
    assert_eq!(orchestrator.state(), RunState::Running);
    assert_eq!(orchestrator.started_runs(), 1);
}

#[test]
fn pause_and_resume_are_symmetric_and_keep_the_run_active() {
    let mut orchestrator = running();
    orchestrator.observe_tick(SimulationTick::new(12)).unwrap();

    assert_eq!(
        orchestrator.request(RunRequest::Pause).unwrap(),
        RunIntent::PauseSession
    );
    assert_eq!(orchestrator.state(), RunState::Paused);
    // A paused run is still active, so frames already in flight are recorded.
    orchestrator.observe_tick(SimulationTick::new(13)).unwrap();
    assert_eq!(orchestrator.tick(), SimulationTick::new(13));

    assert_eq!(
        orchestrator.request(RunRequest::Resume).unwrap(),
        RunIntent::ResumeSession
    );
    assert_eq!(orchestrator.state(), RunState::Running);
}

#[test]
fn cancel_is_requested_but_only_the_backend_ends_the_run() {
    let mut orchestrator = running();

    assert_eq!(
        orchestrator.request(RunRequest::Cancel).unwrap(),
        RunIntent::CancelSession
    );
    // A slow cancel must not look like a finished run.
    assert_eq!(orchestrator.state(), RunState::Running);

    orchestrator.finish(RunOutcome::Cancelled).unwrap();
    assert_eq!(orchestrator.state(), RunState::Cancelled);
}

#[test]
fn every_terminal_outcome_is_reachable_and_final() {
    for (outcome, expected) in [
        (RunOutcome::Completed, RunState::Completed),
        (RunOutcome::Cancelled, RunState::Cancelled),
        (RunOutcome::Failed, RunState::Failed),
    ] {
        let mut orchestrator = running();
        orchestrator.finish(outcome).unwrap();
        assert_eq!(orchestrator.state(), expected);
        assert!(orchestrator.state().is_terminal());

        // A second outcome cannot overwrite the first one.
        let error = orchestrator.finish(RunOutcome::Completed).unwrap_err();
        assert_eq!(error.code(), RunErrorCode::NoActiveRun);
        assert_eq!(orchestrator.state(), expected);

        // Only a reset leaves a terminal state.
        for request in EVERY_REQUEST {
            if request == RunRequest::Reset {
                continue;
            }
            assert!(
                !orchestrator.accepts(request),
                "{request:?} after {outcome:?}"
            );
        }
    }
}

#[test]
fn a_terminal_run_restarts_from_the_same_prepared_artifact() {
    let mut orchestrator = running();
    orchestrator.observe_tick(SimulationTick::new(99)).unwrap();
    orchestrator.finish(RunOutcome::Completed).unwrap();

    assert_eq!(
        orchestrator.request(RunRequest::Reset).unwrap(),
        RunIntent::ReleaseSession
    );
    // Reset returns to Prepared, not Idle: the artifact survives a restart.
    assert_eq!(orchestrator.state(), RunState::Prepared);

    orchestrator.request(RunRequest::Start).unwrap();
    assert_eq!(orchestrator.state(), RunState::Running);
    assert_eq!(orchestrator.started_runs(), 2);
    // The restarted run starts its own timeline.
    assert_eq!(orchestrator.tick(), SimulationTick::new(0));
}

#[test]
fn every_invalid_transition_is_diagnosed_and_changes_nothing() {
    let states: [(RunState, RunOrchestrator); 4] = [
        (RunState::Idle, RunOrchestrator::new()),
        (RunState::Prepared, prepared()),
        (RunState::Running, running()),
        (RunState::Paused, {
            let mut orchestrator = running();
            orchestrator.request(RunRequest::Pause).unwrap();
            orchestrator
        }),
    ];
    for (expected_state, orchestrator) in states {
        for request in EVERY_REQUEST {
            let mut candidate = orchestrator;
            if candidate.accepts(request) {
                continue;
            }
            let error = candidate.request(request).unwrap_err();
            assert_eq!(error.code(), RunErrorCode::InvalidTransition);
            assert_eq!(error.state(), expected_state);
            assert_eq!(error.request(), Some(request));
            assert_eq!(
                candidate, orchestrator,
                "{request:?} must not change {expected_state:?}"
            );
        }
    }
}

#[test]
fn frames_and_state_changes_outside_a_run_are_refused() {
    let mut orchestrator = RunOrchestrator::new();
    assert_eq!(
        orchestrator
            .observe_tick(SimulationTick::new(1))
            .unwrap_err()
            .code(),
        RunErrorCode::NoActiveRun
    );
    assert_eq!(
        orchestrator
            .observe_session_state(SessionState::Running)
            .unwrap_err()
            .code(),
        RunErrorCode::NoActiveRun
    );
    assert_eq!(orchestrator.state(), RunState::Idle);
}

#[test]
fn an_unrequested_backend_state_change_wins_over_the_last_user_action() {
    let mut orchestrator = running();

    // The engine paused itself; the observable state must follow the engine.
    orchestrator
        .observe_session_state(SessionState::Paused)
        .unwrap();
    assert_eq!(orchestrator.state(), RunState::Paused);

    orchestrator
        .observe_session_state(SessionState::Failed)
        .unwrap();
    assert_eq!(orchestrator.state(), RunState::Failed);
}

fn demo_network() -> CompiledNetwork {
    let lanes = LaneTable::new(
        vec![CompiledPoint::new(0.0, 0.0)],
        vec![CompiledPoint::new(100.0, 0.0)],
        vec![3.5],
        vec![CompiledLaneUse::GeneralTraffic],
    )
    .unwrap();
    CompiledNetwork::new(
        CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([3; 32])),
        lanes,
        vec![LaneOrigin::new(
            CorridorId::from_u128(10),
            LaneId::from_u128(11),
        )],
        CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
    )
    .unwrap()
}

#[test]
fn the_state_machine_drives_a_real_backend_session_to_completion() {
    pollster::block_on(async {
        let backend = FakeBackend::new();
        let mut orchestrator = RunOrchestrator::new();

        assert_eq!(
            orchestrator.request(RunRequest::Prepare).unwrap(),
            RunIntent::Compile
        );
        backend.handshake(ClientHello::current()).await.unwrap();
        let artifact = backend
            .compile(
                Arc::new(demo_network()),
                ScenarioSnapshot::new(Sha256Digest::from_bytes([4; 32]), 2).unwrap(),
                CompileOptions,
            )
            .await
            .unwrap();

        assert_eq!(
            orchestrator.request(RunRequest::Start).unwrap(),
            RunIntent::StartSession
        );
        let mut session = backend
            .start(
                artifact,
                RunConfig::new(
                    Sha256Digest::from_bytes([4; 32]),
                    4,
                    1,
                    RootSeed::new(7),
                    SeedAlgorithmVersion::V1,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        // Every backend event is mirrored into the observable state.
        let mut last_frame_tick = None;
        loop {
            match session.next_event().await.unwrap() {
                roadsim_backend_api::BackendEvent::Frame(frame) => {
                    orchestrator.observe_tick(frame.tick()).unwrap();
                    last_frame_tick = Some(frame.tick());
                }
                roadsim_backend_api::BackendEvent::StateChanged(state) => {
                    orchestrator.observe_session_state(state).unwrap();
                }
                roadsim_backend_api::BackendEvent::Completed(_) => break,
            }
            if orchestrator.state().is_terminal() {
                break;
            }
        }
        if !orchestrator.state().is_terminal() {
            orchestrator.finish(RunOutcome::Completed).unwrap();
        }

        assert_eq!(orchestrator.state(), RunState::Completed);
        // The observable tick is the last frame the backend actually emitted.
        assert_eq!(Some(orchestrator.tick()), last_frame_tick);
        assert!(orchestrator.tick() > SimulationTick::new(0));

        // A pause request against a finished run is refused, not sent on.
        assert_eq!(
            orchestrator.request(RunRequest::Pause).unwrap_err().code(),
            RunErrorCode::InvalidTransition
        );
        assert!(
            session
                .control(ControlCommand::new(1, 1, ControlKind::Pause))
                .await
                .is_err()
        );
    });
}
