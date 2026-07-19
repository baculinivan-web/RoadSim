use roadsim_worker_client::{WorkerClient, WorkerClientConfig, WorkerClientErrorCode};
use roadsim_worker_protocol::{
    AuthToken, RequestEnvelope, RequestPayload, ResponseEnvelope, ResponsePayload,
    WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDiagnosticCode, read_frame, write_frame,
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

#[test]
fn handshake_ping_session_cancel_and_shutdown_complete() {
    let mut client = WorkerClient::spawn(config(token("a"))).unwrap();
    let hello = client
        .handshake(vec!["worker.stub.lifecycle".to_owned()], TIMEOUT)
        .unwrap();
    assert_eq!(hello.worker_name, "roadsim-worker-stub");
    assert_eq!(hello.capabilities, ["worker.stub.lifecycle"]);
    client.ping(TIMEOUT).unwrap();
    client.open_session(42, TIMEOUT).unwrap();
    client.cancel_session(42, TIMEOUT).unwrap();
    client.shutdown(TIMEOUT).unwrap();
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
        RequestPayload::OpenSession,
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
