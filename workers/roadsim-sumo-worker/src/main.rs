mod native;
mod pin;

use native::{NativeEngine, NativeErrorCode};
use roadsim_worker_protocol::{
    AuthToken, MAX_STEPS_PER_REQUEST, RequestEnvelope, RequestPayload, ResponseEnvelope,
    ResponsePayload, WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDiagnosticCode,
    capabilities_are_valid, read_frame, write_frame,
};
use std::{env, ffi::OsStr, io, path::PathBuf, process::ExitCode};

const LIFECYCLE_CAPABILITY: &str = "simulation.lifecycle.step.v1";

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    if arguments.next().as_deref() != Some(OsStr::new("--bridge")) {
        return ExitCode::from(64);
    }
    let Some(bridge_path) = arguments.next() else {
        return ExitCode::from(64);
    };
    if arguments.next().is_some() {
        return ExitCode::from(64);
    }
    let Ok(expected_token) = env::var(WORKER_TOKEN_ENV) else {
        return ExitCode::from(78);
    };
    let Ok(expected_token) = AuthToken::parse(expected_token) else {
        return ExitCode::from(78);
    };
    run(
        expected_token,
        NativeEngine::load(&PathBuf::from(bridge_path)),
    )
}

fn run(expected_token: AuthToken, mut engine: Result<NativeEngine, NativeErrorCode>) -> ExitCode {
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
            let _ = respond_error(
                &mut writer,
                &request,
                WorkerDiagnosticCode::ProtocolVersionMismatch,
                Vec::new(),
            );
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
        if !session_shape_is_valid(&request) {
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
            required_engine,
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
            let Ok(loaded_engine) = engine.as_ref() else {
                let _ = respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::EngineUnavailable,
                    Vec::new(),
                );
                return ExitCode::from(69);
            };
            let Ok(identity) = loaded_engine.identity() else {
                let _ = respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::EngineUnavailable,
                    Vec::new(),
                );
                return ExitCode::from(69);
            };
            if identity != pin::identity() {
                let _ = respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::EngineIdentityMismatch,
                    Vec::new(),
                );
                return ExitCode::from(69);
            }
            if required_engine
                .as_ref()
                .is_some_and(|required| required != &identity)
            {
                if respond_error(
                    &mut writer,
                    &request,
                    WorkerDiagnosticCode::EngineIdentityMismatch,
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
            let unsupported = required_capabilities
                .into_iter()
                .filter(|capability| capability != LIFECYCLE_CAPABILITY)
                .collect::<Vec<_>>();
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
                        worker_name: "sumo-worker".to_owned(),
                        worker_version: env!("CARGO_PKG_VERSION").to_owned(),
                        engine: identity,
                        capabilities: vec![LIFECYCLE_CAPABILITY.to_owned()],
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

        let payload = match request.payload.clone() {
            RequestPayload::Ping => ResponsePayload::Pong,
            RequestPayload::OpenSession { config } => {
                if active_session.is_some() {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionAlreadyActive,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                if !config.is_valid() {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionConfigInvalid,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                let Some(session_id) = request.session_id else {
                    return ExitCode::from(65);
                };
                let Ok(loaded_engine) = engine.as_mut() else {
                    return ExitCode::from(69);
                };
                if loaded_engine
                    .start(
                        config.bundle_path(),
                        config.root_seed(),
                        config.step_length_ms(),
                    )
                    .is_err()
                {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::EngineStartFailed,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                active_session = Some(session_id);
                ResponsePayload::SessionOpened
            }
            RequestPayload::StepSession { steps } => {
                if active_session != request.session_id || active_session.is_none() {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionNotFound,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                if steps == 0 || steps > MAX_STEPS_PER_REQUEST {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionStepInvalid,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                let Ok(loaded_engine) = engine.as_mut() else {
                    return ExitCode::from(69);
                };
                let tick = match loaded_engine.step(steps) {
                    Ok(tick) => tick,
                    Err(_) => {
                        if !respond_and_continue(
                            &mut writer,
                            &request,
                            WorkerDiagnosticCode::EngineStepFailed,
                        ) {
                            return ExitCode::from(74);
                        }
                        continue;
                    }
                };
                ResponsePayload::SessionStepped { tick }
            }
            RequestPayload::CloseSession | RequestPayload::CancelSession => {
                if active_session != request.session_id || active_session.is_none() {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::SessionNotFound,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                let Ok(loaded_engine) = engine.as_mut() else {
                    return ExitCode::from(69);
                };
                if loaded_engine.close().is_err() {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::EngineCloseFailed,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                active_session = None;
                if matches!(&request.payload, RequestPayload::CancelSession) {
                    ResponsePayload::SessionCancelled
                } else {
                    ResponsePayload::SessionClosed
                }
            }
            RequestPayload::Shutdown => {
                let close_failed = if active_session.is_some() {
                    let Ok(loaded_engine) = engine.as_mut() else {
                        return ExitCode::from(69);
                    };
                    loaded_engine.close().is_err()
                } else {
                    false
                };
                if close_failed {
                    if !respond_and_continue(
                        &mut writer,
                        &request,
                        WorkerDiagnosticCode::EngineCloseFailed,
                    ) {
                        return ExitCode::from(74);
                    }
                    continue;
                }
                ResponsePayload::ShutdownAcknowledged
            }
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

fn session_shape_is_valid(request: &RequestEnvelope) -> bool {
    match &request.payload {
        RequestPayload::OpenSession { .. }
        | RequestPayload::StepSession { .. }
        | RequestPayload::CloseSession
        | RequestPayload::CancelSession => request.session_id.is_some(),
        RequestPayload::Handshake { .. } | RequestPayload::Ping | RequestPayload::Shutdown => {
            request.session_id.is_none()
        }
    }
}

fn respond_and_continue(
    writer: &mut impl io::Write,
    request: &RequestEnvelope,
    code: WorkerDiagnosticCode,
) -> bool {
    respond_error(writer, request, code, Vec::new()).is_ok()
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
