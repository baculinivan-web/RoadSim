use roadsim_worker_client::{
    ReliableWorkerEvent, RunDirectoryLimits, RunDirectoryManager, RunState, WorkerClient,
    WorkerClientConfig, WorkerClientErrorCode,
};
use roadsim_worker_protocol::{
    AuthToken, EngineIdentity, RequestEnvelope, RequestPayload, ResponseEnvelope, ResponsePayload,
    TerminalStatus, WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDiagnosticCode,
    WorkerSessionConfig, read_frame, write_frame,
};
use std::{
    io::BufReader,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(2);

fn token(character: &str) -> AuthToken {
    AuthToken::parse(character.repeat(64)).unwrap()
}

fn config(auth_token: AuthToken) -> WorkerClientConfig {
    WorkerClientConfig::new(env!("CARGO_BIN_EXE_roadsim-worker-stub"), auth_token)
}

fn session_config() -> WorkerSessionConfig {
    WorkerSessionConfig::new("bundle/run.sumocfg", 42, 100).unwrap()
}

#[test]
fn handshake_ping_session_cancel_and_shutdown_complete() {
    let mut client = WorkerClient::spawn(config(token("a"))).unwrap();
    let hello = client
        .handshake(vec!["worker.stub.lifecycle".to_owned()], TIMEOUT)
        .unwrap();
    assert_eq!(hello.worker_name, "roadsim-worker-stub");
    assert_eq!(hello.worker_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(hello.engine.name(), "roadsim.stub");
    assert_eq!(hello.engine.version(), "1");
    assert_eq!(hello.engine.build_revision(), "builtin-v1");
    assert_eq!(hello.capabilities, ["worker.stub.lifecycle"]);
    client.ping(TIMEOUT).unwrap();
    client.open_session(42, session_config(), TIMEOUT).unwrap();
    assert_eq!(client.step_session(42, 3, TIMEOUT).unwrap(), 3);
    assert_eq!(client.step_session(42, 2, TIMEOUT).unwrap(), 5);
    client.close_session(42, TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();
}

#[test]
fn invalid_step_count_is_rejected_without_changing_session_tick() {
    let mut client = WorkerClient::spawn(config(token("d"))).unwrap();
    client.handshake(Vec::new(), TIMEOUT).unwrap();
    client.open_session(8, session_config(), TIMEOUT).unwrap();
    let error = client.step_session(8, 0, TIMEOUT).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::InvalidStepCount);
    assert_eq!(client.step_session(8, 1, TIMEOUT).unwrap(), 1);
    client.close_session(8, TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();
}

#[test]
fn exact_engine_requirement_is_checked_before_session_open() {
    let mut accepted = WorkerClient::spawn(config(token("a"))).unwrap();
    let required = EngineIdentity::new("roadsim.stub", "1", "builtin-v1").unwrap();
    let hello = accepted
        .handshake_with_engine(Vec::new(), required.clone(), TIMEOUT)
        .unwrap();
    assert_eq!(hello.engine, required);
    accepted.shutdown(TIMEOUT).unwrap();

    let mut rejected = WorkerClient::spawn(config(token("b"))).unwrap();
    let wrong = EngineIdentity::new("eclipse.sumo", "1.27.1", "7717f2379d9e").unwrap();
    let error = rejected
        .handshake_with_engine(Vec::new(), wrong, TIMEOUT)
        .unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerRejected);
    assert_eq!(
        error.worker_diagnostic(),
        Some(WorkerDiagnosticCode::EngineIdentityMismatch)
    );
}

#[test]
fn version_mismatch_is_explicitly_rejected() {
    let mut client = WorkerClient::spawn(config(token("a")).with_protocol_version(99)).unwrap();
    let error = client.handshake(Vec::new(), TIMEOUT).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerRejected);
    assert_eq!(
        error.worker_diagnostic(),
        Some(WorkerDiagnosticCode::ProtocolVersionMismatch)
    );
}

#[test]
fn unknown_capability_is_rejected_before_session_open() {
    let mut client = WorkerClient::spawn(config(token("a"))).unwrap();
    let error = client
        .handshake(vec!["simulation.unknown".to_owned()], TIMEOUT)
        .unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerRejected);
    assert_eq!(
        error.worker_diagnostic(),
        Some(WorkerDiagnosticCode::UnsupportedCapability)
    );
    assert_eq!(error.unsupported_capabilities(), ["simulation.unknown"]);
}

#[test]
fn wrong_handshake_token_is_rejected_and_never_echoed() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_roadsim-worker-stub"))
        .env(WORKER_TOKEN_ENV, token("a").expose_for_transport())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let request = RequestEnvelope::new(
        WORKER_PROTOCOL_VERSION,
        1,
        None,
        1,
        RequestPayload::Handshake {
            auth_token: token("b"),
            required_capabilities: Vec::new(),
            required_engine: None,
        },
    );
    write_frame(child.stdin.as_mut().unwrap(), &request).unwrap();
    let response: ResponseEnvelope =
        read_frame(&mut BufReader::new(child.stdout.take().unwrap())).unwrap();
    assert_eq!(
        response.payload,
        ResponsePayload::Error {
            code: WorkerDiagnosticCode::AuthenticationFailed,
            unsupported_capabilities: Vec::new(),
        }
    );
    let status = child.wait().unwrap();
    assert!(!status.success());
}

#[test]
fn session_request_before_handshake_is_rejected() {
    let auth_token = token("a");
    let mut child = Command::new(env!("CARGO_BIN_EXE_roadsim-worker-stub"))
        .env(WORKER_TOKEN_ENV, auth_token.expose_for_transport())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let request = RequestEnvelope::new(
        WORKER_PROTOCOL_VERSION,
        1,
        Some(42),
        1,
        RequestPayload::OpenSession {
            config: session_config(),
        },
    );
    write_frame(child.stdin.as_mut().unwrap(), &request).unwrap();
    let response: ResponseEnvelope =
        read_frame(&mut BufReader::new(child.stdout.take().unwrap())).unwrap();
    assert_eq!(
        response.payload,
        ResponsePayload::Error {
            code: WorkerDiagnosticCode::HandshakeRequired,
            unsupported_capabilities: Vec::new(),
        }
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn worker_crash_is_isolated_as_process_exit() {
    let mut client =
        WorkerClient::spawn(config(token("a")).with_argument("--crash-on-ping")).unwrap();
    client.handshake(Vec::new(), TIMEOUT).unwrap();
    let error = client.ping(TIMEOUT).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::WorkerExited);
}

#[test]
fn hung_worker_is_killed_at_request_timeout() {
    let mut client =
        WorkerClient::spawn(config(token("a")).with_argument("--hang-on-ping")).unwrap();
    client.handshake(Vec::new(), TIMEOUT).unwrap();
    let started = Instant::now();
    let error = client.ping(Duration::from_millis(100)).unwrap_err();
    assert_eq!(error.code(), WorkerClientErrorCode::Timeout);
    assert!(started.elapsed() < TIMEOUT);
}

#[test]
fn visual_frames_drop_to_latest_while_metrics_and_terminal_remain_reliable() {
    let mut client =
        WorkerClient::spawn(config(token("a")).with_argument("--emit-batches-on-ping")).unwrap();
    let hello = client
        .handshake(
            vec![
                "worker.stub.lifecycle".to_owned(),
                "worker.stub.batches".to_owned(),
            ],
            TIMEOUT,
        )
        .unwrap();
    assert!(
        hello
            .capabilities
            .contains(&"worker.stub.batches".to_owned())
    );
    client.open_session(42, session_config(), TIMEOUT).unwrap();
    client.ping(TIMEOUT).unwrap();
    client.cancel_session(42, TIMEOUT).unwrap();

    let mut observed_ticks = Vec::new();
    for _ in 0..12 {
        let event = client.recv_reliable_event_timeout(TIMEOUT).unwrap();
        let ReliableWorkerEvent::MetricBatch { batch, .. } = event else {
            panic!("metric batch expected before terminal event");
        };
        observed_ticks.push(batch.samples()[0].tick());
        assert_eq!(batch.samples()[0].definition_version(), "1.0.0");
    }
    assert_eq!(observed_ticks, (0..12).collect::<Vec<_>>());

    let (session_id, sequence, visual) = client.take_latest_visual_frame().unwrap();
    assert_eq!(session_id, 42);
    assert_eq!(visual.tick(), 31);
    assert_eq!(visual.agent_ids(), [0, 1]);
    assert_eq!(sequence, 32);
    assert_eq!(client.dropped_visual_frames(), 31);
    assert_eq!(client.data_stream_error(), None);

    let event = client.recv_reliable_event_timeout(TIMEOUT).unwrap();
    let ReliableWorkerEvent::Terminal { event, .. } = event else {
        panic!("terminal event expected after cancellation");
    };
    assert_eq!(event.status, TerminalStatus::Cancelled);
    client.shutdown(TIMEOUT).unwrap();
}

#[test]
fn graceful_shutdown_preserves_reliable_batches_already_in_the_pipe() {
    let mut client =
        WorkerClient::spawn(config(token("a")).with_argument("--emit-batches-on-ping")).unwrap();
    client
        .handshake(vec!["worker.stub.batches".to_owned()], TIMEOUT)
        .unwrap();
    client.open_session(77, session_config(), TIMEOUT).unwrap();
    client.ping(TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();

    let (session_id, _, visual) = client.take_latest_visual_frame().unwrap();
    assert_eq!(session_id, 77);
    assert_eq!(visual.tick(), 31);
    let metrics: Vec<_> = (0..12)
        .map(|_| client.try_recv_reliable_event().unwrap())
        .collect();
    assert!(metrics.iter().all(|event| matches!(
        event,
        ReliableWorkerEvent::MetricBatch { session_id: 77, .. }
    )));
}

#[test]
fn worker_starts_inside_owned_run_directory_and_journals_completion() {
    let root = std::env::temp_dir().join(format!(
        "roadsim-worker-workdir-test-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let limits = RunDirectoryLimits::new(2, 8).unwrap();
    let manager = RunDirectoryManager::new(&root, limits).unwrap();
    let mut run = manager.create_run(91, 1).unwrap();
    run.mark_running().unwrap();

    let mut client = WorkerClient::spawn(
        config(token("a"))
            .with_argument("--require-workdir")
            .with_work_directory(run.path()),
    )
    .unwrap();
    client.handshake(Vec::new(), TIMEOUT).unwrap();
    client.ping(TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();
    run.mark_completed().unwrap();
    drop(run);

    let records = manager.recover().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state(), RunState::Completed);
    std::fs::remove_dir_all(&root).unwrap();
}
