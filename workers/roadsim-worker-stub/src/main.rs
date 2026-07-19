use roadsim_worker_protocol::{
    AuthToken, RequestEnvelope, RequestPayload, ResponseEnvelope, ResponsePayload,
    WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDiagnosticCode, capabilities_are_valid,
    read_frame, write_frame,
};
use std::{env, io, process::ExitCode};

const SUPPORTED_CAPABILITY: &str = "worker.stub.lifecycle";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Normal,
    CrashOnPing,
    HangOnPing,
}

fn main() -> ExitCode {
    let mode = match env::args_os().nth(1).as_deref() {
        None => Mode::Normal,
        Some(value) if value == "--crash-on-ping" => Mode::CrashOnPing,
        Some(value) if value == "--hang-on-ping" => Mode::HangOnPing,
        Some(_) => return ExitCode::from(64),
    };
    let Ok(expected_token) = env::var(WORKER_TOKEN_ENV) else {
        return ExitCode::from(78);
    };
    let Ok(expected_token) = AuthToken::parse(expected_token) else {
        return ExitCode::from(78);
    };
    run(expected_token, mode)
}

fn run(expected_token: AuthToken, mode: Mode) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut handshake_complete = false;
    let mut last_sequence = None;
    let mut active_session = None;

    loop {
        let request: RequestEnvelope = match read_frame(&mut reader) {
            Ok(request) => request,
            Err(error) if error.code() == roadsim_worker_protocol::FrameErrorCode::EndOfStream => {
                return ExitCode::SUCCESS;
            }
            Err(_) => return ExitCode::from(65),
        };

        if request.protocol_version != WORKER_PROTOCOL_VERSION {
            if respond_error(
                &mut writer,
                &request,
                WorkerDiagnosticCode::ProtocolVersionMismatch,
                Vec::new(),
            )
            .is_err()
            {
                return ExitCode::from(74);
            }
            return ExitCode::from(76);
        }
        if last_sequence.is_some_and(|last| request.sequence <= last) {
            if respond_error(
                &mut writer,
                &request,
                WorkerDiagnosticCode::SequenceOutOfOrder,
                Vec::new(),
            )
            .is_err()
            {
                return ExitCode::from(74);
            }
            continue;
        }
        last_sequence = Some(request.sequence);

        let session_shape_is_valid = match &request.payload {
            RequestPayload::OpenSession | RequestPayload::CancelSession => {
                request.session_id.is_some()
            }
            RequestPayload::Handshake { .. } | RequestPayload::Ping | RequestPayload::Shutdown => {
                request.session_id.is_none()
            }
        };
        if !session_shape_is_valid {
            if respond_error(
                &mut writer,
                &request,
                WorkerDiagnosticCode::InvalidRequest,
                Vec::new(),
            )
            .is_err()
            {
                return ExitCode::from(74);
            }
            continue;
        }

        if let RequestPayload::Handshake {
            auth_token,
            mut required_capabilities,
        } = request.payload.clone()
        {
            if handshake_complete {
                if respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::InvalidRequest,
                    Vec::new(),
                )
                .is_err()
                {
                    return ExitCode::from(74);
                }
                continue;
            }
            if !expected_token.matches(&auth_token) {
                let _ = respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::AuthenticationFailed,
                    Vec::new(),
                );
                return ExitCode::from(77);
            }
            if !capabilities_are_valid(&required_capabilities) {
                if respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::InvalidCapabilityManifest,
                    Vec::new(),
                )
                .is_err()
                {
                    return ExitCode::from(74);
                }
                continue;
            }
            required_capabilities.sort_unstable();
            required_capabilities.dedup();
            let unsupported: Vec<_> = required_capabilities
                .into_iter()
                .filter(|capability| capability != SUPPORTED_CAPABILITY)
                .collect();
            if !unsupported.is_empty() {
                if respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::UnsupportedCapability,
                    unsupported,
                )
                .is_err()
                {
                    return ExitCode::from(74);
                }
                continue;
            }
            handshake_complete = true;
            if write_frame(
                &mut writer,
                &ResponseEnvelope::new(
                    &request,
                    ResponsePayload::HandshakeAccepted {
                        worker_name: "roadsim-worker-stub".to_owned(),
                        capabilities: vec![SUPPORTED_CAPABILITY.to_owned()],
                    },
                ),
            )
            .is_err()
            {
                return ExitCode::from(74);
            }
            continue;
        }

        if !handshake_complete {
            if respond_error(
                &mut writer,
                &request,
                WorkerDiagnosticCode::HandshakeRequired,
                Vec::new(),
            )
            .is_err()
            {
                return ExitCode::from(74);
            }
            continue;
        }

        let payload = match request.payload {
            RequestPayload::Ping => match mode {
                Mode::Normal => ResponsePayload::Pong,
                Mode::CrashOnPing => return ExitCode::from(70),
                Mode::HangOnPing => loop {
                    std::thread::park();
                },
            },
            RequestPayload::OpenSession => {
                let Some(session_id) = request.session_id else {
                    if respond_error(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::InvalidRequest,
                        Vec::new(),
                    )
                    .is_err()
                    {
                        return ExitCode::from(74);
                    }
                    continue;
                };
                if active_session.is_some() {
                    if respond_error(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionAlreadyActive,
                        Vec::new(),
                    )
                    .is_err()
                    {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                active_session = Some(session_id);
                ResponsePayload::SessionOpened
            }
            RequestPayload::CancelSession => {
                if active_session != request.session_id || active_session.is_none() {
                    if respond_error(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionNotFound,
                        Vec::new(),
                    )
                    .is_err()
                    {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                active_session = None;
                ResponsePayload::SessionCancelled
            }
            RequestPayload::Shutdown => ResponsePayload::ShutdownAcknowledged,
            RequestPayload::Handshake { .. } => unreachable!("handshake handled above"),
        };
        let shutdown = payload == ResponsePayload::ShutdownAcknowledged;
        if write_frame(&mut writer, &ResponseEnvelope::new(&request, payload)).is_err() {
            return ExitCode::from(74);
        }
        if shutdown {
            return ExitCode::SUCCESS;
        }
    }
}

fn respond_error(
    writer: &mut impl io::Write,
    request: &RequestEnvelope,
    code: WorkerDiagnosticCode,
    unsupported_capabilities: Vec<String>,
) -> Result<(), roadsim_worker_protocol::FrameError> {
    write_frame(
        writer,
        &ResponseEnvelope::new(
            request,
            ResponsePayload::Error {
                code,
                unsupported_capabilities,
            },
        ),
    )
}
