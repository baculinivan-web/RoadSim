use roadsim_worker_protocol::{
    AuthToken, MetricBatch, MetricSample, RequestEnvelope, RequestPayload, ResponseEnvelope,
    ResponsePayload, TerminalEvent, TerminalStatus, VisualFrameBatch, WORKER_PROTOCOL_VERSION,
    WORKER_TOKEN_ENV, WorkerDataEnvelope, WorkerDataPayload, WorkerDiagnosticCode,
    capabilities_are_valid, read_frame, write_data_frame, write_frame,
};
use std::{env, io, process::ExitCode, sync::mpsc, thread};

const SUPPORTED_CAPABILITY: &str = "worker.stub.lifecycle";
const BATCH_CAPABILITY: &str = "worker.stub.batches";
const DATA_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Normal,
    CrashOnPing,
    HangOnPing,
    EmitBatchesOnPing,
    RequireWorkdir,
}

fn main() -> ExitCode {
    let mode = match env::args_os().nth(1).as_deref() {
        None => Mode::Normal,
        Some(value) if value == "--crash-on-ping" => Mode::CrashOnPing,
        Some(value) if value == "--hang-on-ping" => Mode::HangOnPing,
        Some(value) if value == "--emit-batches-on-ping" => Mode::EmitBatchesOnPing,
        Some(value) if value == "--require-workdir" => Mode::RequireWorkdir,
        Some(_) => return ExitCode::from(64),
    };
    if mode == Mode::RequireWorkdir && !workdir_marker_is_present() {
        return ExitCode::from(72);
    }
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
    let (data_sender, data_receiver) = mpsc::sync_channel(DATA_QUEUE_CAPACITY);
    let data_writer_thread = thread::spawn(move || {
        let stderr = io::stderr();
        let mut data_writer = stderr.lock();
        while let Ok(envelope) = data_receiver.recv() {
            if write_data_frame(&mut data_writer, &envelope).is_err() {
                break;
            }
        }
    });
    let mut handshake_complete = false;
    let mut last_sequence = None;
    let mut active_session = None;
    let mut data_sequence = 1_u64;

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
                .filter(|capability| !supported_capabilities(mode).contains(&capability.as_str()))
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
                        capabilities: supported_capabilities(mode)
                            .iter()
                            .map(|capability| (*capability).to_owned())
                            .collect(),
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
                Mode::EmitBatchesOnPing => {
                    let Some(session_id) = active_session else {
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
                    };
                    if emit_demo_batches(&data_sender, &mut data_sequence, session_id).is_err() {
                        return ExitCode::from(74);
                    }
                    ResponsePayload::Pong
                }
                Mode::RequireWorkdir => ResponsePayload::Pong,
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
                if data_sender
                    .send(WorkerDataEnvelope::new(
                        request.session_id.unwrap_or_default(),
                        data_sequence,
                        WorkerDataPayload::Terminal(TerminalEvent {
                            status: TerminalStatus::Cancelled,
                            diagnostic: None,
                        }),
                    ))
                    .is_err()
                {
                    return ExitCode::from(74);
                }
                data_sequence = data_sequence.saturating_add(1);
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
            drop(data_sender);
            let _ = data_writer_thread.join();
            return ExitCode::SUCCESS;
        }
    }
}

fn workdir_marker_is_present() -> bool {
    let Ok(current_directory) = env::current_dir() else {
        return false;
    };
    let Some(name) = current_directory.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.len() == 37
        && name.starts_with("run-")
        && name.as_bytes().get(20) == Some(&b'-')
        && current_directory.join("state-00000000.json").is_file()
}

fn supported_capabilities(mode: Mode) -> &'static [&'static str] {
    const LIFECYCLE: &[&str] = &[SUPPORTED_CAPABILITY];
    const BATCHES: &[&str] = &[SUPPORTED_CAPABILITY, BATCH_CAPABILITY];
    if mode == Mode::EmitBatchesOnPing {
        BATCHES
    } else {
        LIFECYCLE
    }
}

fn emit_demo_batches(
    sender: &mpsc::SyncSender<WorkerDataEnvelope>,
    sequence: &mut u64,
    session_id: u64,
) -> Result<(), ()> {
    for tick in 0..32_u64 {
        let batch = VisualFrameBatch::new(
            tick,
            vec![0, 1],
            vec![tick as f64, tick as f64 + 4.5],
            vec![0.0, 0.0],
            vec![0.0, 0.0],
            vec![4.5, 4.5],
            vec![1.8, 1.8],
        )
        .map_err(|_| ())?;
        sender
            .send(WorkerDataEnvelope::new(
                session_id,
                *sequence,
                WorkerDataPayload::VisualFrame(batch),
            ))
            .map_err(|_| ())?;
        *sequence = (*sequence).saturating_add(1);
    }
    for tick in 0..12_u64 {
        let sample = MetricSample::new(tick, "stub.completed_agents", "1.0.0", tick as f64)
            .map_err(|_| ())?;
        let batch = MetricBatch::new(vec![sample]).map_err(|_| ())?;
        sender
            .send(WorkerDataEnvelope::new(
                session_id,
                *sequence,
                WorkerDataPayload::MetricBatch(batch),
            ))
            .map_err(|_| ())?;
        *sequence = (*sequence).saturating_add(1);
    }
    Ok(())
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
