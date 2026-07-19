//! Cross-platform child-process lifecycle for RoadSim workers.
//!
//! The client uses inherited standard-I/O pipes and never opens a listener. One
//! mutable client permits one in-flight request, making response correlation and
//! ordering explicit without an async runtime.

use roadsim_worker_protocol::{
    AuthToken, FrameError, RequestEnvelope, RequestPayload, ResponseEnvelope, ResponsePayload,
    WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDiagnosticCode, capabilities_are_valid,
    read_frame, write_frame,
};
use std::{
    error::Error,
    ffi::OsString,
    fmt,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RESPONSE_QUEUE_CAPACITY: usize = 8;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Clone, Debug)]
pub struct WorkerClientConfig {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub auth_token: AuthToken,
    pub protocol_version: u32,
}

impl WorkerClientConfig {
    #[must_use]
    pub fn new(program: impl Into<PathBuf>, auth_token: AuthToken) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
            auth_token,
            protocol_version: WORKER_PROTOCOL_VERSION,
        }
    }

    #[must_use]
    pub fn with_argument(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    #[must_use]
    pub const fn with_protocol_version(mut self, protocol_version: u32) -> Self {
        self.protocol_version = protocol_version;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerClientErrorCode {
    SpawnFailed,
    MissingPipe,
    FrameWriteFailed,
    FrameReadFailed,
    Timeout,
    WorkerExited,
    CorrelationMismatch,
    InvalidCapabilityManifest,
    WorkerRejected,
    UnexpectedResponse,
}

impl WorkerClientErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpawnFailed => "worker.client.spawn_failed",
            Self::MissingPipe => "worker.client.pipe_missing",
            Self::FrameWriteFailed => "worker.client.frame_write_failed",
            Self::FrameReadFailed => "worker.client.frame_read_failed",
            Self::Timeout => "worker.client.timeout",
            Self::WorkerExited => "worker.client.exited",
            Self::CorrelationMismatch => "worker.client.correlation_mismatch",
            Self::InvalidCapabilityManifest => "worker.client.capability_manifest_invalid",
            Self::WorkerRejected => "worker.client.rejected",
            Self::UnexpectedResponse => "worker.client.response_unexpected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerClientError {
    code: WorkerClientErrorCode,
    worker_diagnostic: Option<WorkerDiagnosticCode>,
    unsupported_capabilities: Vec<String>,
}

impl WorkerClientError {
    fn new(code: WorkerClientErrorCode) -> Self {
        Self {
            code,
            worker_diagnostic: None,
            unsupported_capabilities: Vec::new(),
        }
    }

    fn rejected(
        worker_diagnostic: WorkerDiagnosticCode,
        unsupported_capabilities: Vec<String>,
    ) -> Self {
        Self {
            code: WorkerClientErrorCode::WorkerRejected,
            worker_diagnostic: Some(worker_diagnostic),
            unsupported_capabilities,
        }
    }

    #[must_use]
    pub const fn code(&self) -> WorkerClientErrorCode {
        self.code
    }

    #[must_use]
    pub const fn worker_diagnostic(&self) -> Option<WorkerDiagnosticCode> {
        self.worker_diagnostic
    }

    #[must_use]
    pub fn unsupported_capabilities(&self) -> &[String] {
        &self.unsupported_capabilities
    }
}

impl fmt::Display for WorkerClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())?;
        if let Some(diagnostic) = self.worker_diagnostic {
            write!(formatter, ": {}", diagnostic.as_str())?;
        }
        Ok(())
    }
}

impl Error for WorkerClientError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerHello {
    pub worker_name: String,
    pub capabilities: Vec<String>,
}

pub struct WorkerClient {
    child: Child,
    writer: Option<ChildStdin>,
    responses: Receiver<Result<ResponseEnvelope, FrameError>>,
    reader_thread: Option<JoinHandle<()>>,
    auth_token: AuthToken,
    protocol_version: u32,
    next_request_id: u64,
    next_sequence: u64,
    stopped: bool,
}

impl WorkerClient {
    pub fn spawn(config: WorkerClientConfig) -> Result<Self, WorkerClientError> {
        let mut child = Command::new(&config.program)
            .args(&config.arguments)
            .env(WORKER_TOKEN_ENV, config.auth_token.expose_for_transport())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| WorkerClientError::new(WorkerClientErrorCode::SpawnFailed))?;

        let Some(writer) = child.stdin.take() else {
            terminate_child(&mut child);
            return Err(WorkerClientError::new(WorkerClientErrorCode::MissingPipe));
        };
        let Some(mut reader) = child.stdout.take() else {
            terminate_child(&mut child);
            return Err(WorkerClientError::new(WorkerClientErrorCode::MissingPipe));
        };
        let (sender, responses) = mpsc::sync_channel(RESPONSE_QUEUE_CAPACITY);
        let reader_thread = thread::spawn(move || {
            loop {
                let response = read_frame::<_, ResponseEnvelope>(&mut reader);
                let terminal = response.is_err();
                match sender.try_send(response) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => break,
                }
                if terminal {
                    break;
                }
            }
        });

        Ok(Self {
            child,
            writer: Some(writer),
            responses,
            reader_thread: Some(reader_thread),
            auth_token: config.auth_token,
            protocol_version: config.protocol_version,
            next_request_id: 1,
            next_sequence: 1,
            stopped: false,
        })
    }

    pub fn handshake(
        &mut self,
        required_capabilities: Vec<String>,
        timeout: Duration,
    ) -> Result<WorkerHello, WorkerClientError> {
        if !capabilities_are_valid(&required_capabilities) {
            return Err(WorkerClientError::new(
                WorkerClientErrorCode::InvalidCapabilityManifest,
            ));
        }
        let response = self.request(
            None,
            RequestPayload::Handshake {
                auth_token: self.auth_token.clone(),
                required_capabilities,
            },
            timeout,
        )?;
        match response {
            ResponsePayload::HandshakeAccepted {
                worker_name,
                capabilities,
            } if !worker_name.is_empty()
                && worker_name.len() <= 128
                && capabilities_are_valid(&capabilities) =>
            {
                Ok(WorkerHello {
                    worker_name,
                    capabilities,
                })
            }
            _ => self.unexpected_response(),
        }
    }

    pub fn ping(&mut self, timeout: Duration) -> Result<(), WorkerClientError> {
        match self.request(None, RequestPayload::Ping, timeout)? {
            ResponsePayload::Pong => Ok(()),
            _ => self.unexpected_response(),
        }
    }

    pub fn open_session(
        &mut self,
        session_id: u64,
        timeout: Duration,
    ) -> Result<(), WorkerClientError> {
        match self.request(Some(session_id), RequestPayload::OpenSession, timeout)? {
            ResponsePayload::SessionOpened => Ok(()),
            _ => self.unexpected_response(),
        }
    }

    pub fn cancel_session(
        &mut self,
        session_id: u64,
        timeout: Duration,
    ) -> Result<(), WorkerClientError> {
        match self.request(Some(session_id), RequestPayload::CancelSession, timeout)? {
            ResponsePayload::SessionCancelled => Ok(()),
            _ => self.unexpected_response(),
        }
    }

    pub fn shutdown(&mut self, timeout: Duration) -> Result<(), WorkerClientError> {
        let response = self.request(None, RequestPayload::Shutdown, timeout)?;
        if response != ResponsePayload::ShutdownAcknowledged {
            return self.unexpected_response();
        }
        self.writer.take();
        if !wait_for_exit(&mut self.child, timeout) {
            self.stop_process();
            return Err(WorkerClientError::new(WorkerClientErrorCode::Timeout));
        }
        self.stopped = true;
        self.join_reader();
        Ok(())
    }

    fn request(
        &mut self,
        session_id: Option<u64>,
        payload: RequestPayload,
        timeout: Duration,
    ) -> Result<ResponsePayload, WorkerClientError> {
        if self.stopped {
            return Err(WorkerClientError::new(WorkerClientErrorCode::WorkerExited));
        }
        let request = RequestEnvelope::new(
            self.protocol_version,
            self.next_request_id,
            session_id,
            self.next_sequence,
            payload,
        );
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.next_sequence = self.next_sequence.saturating_add(1);
        let Some(writer) = self.writer.as_mut() else {
            return Err(WorkerClientError::new(WorkerClientErrorCode::WorkerExited));
        };
        if write_frame(writer, &request).is_err() {
            self.stop_process();
            return Err(WorkerClientError::new(
                WorkerClientErrorCode::FrameWriteFailed,
            ));
        }

        let response = match self.responses.recv_timeout(timeout) {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                let code = if error.code() == roadsim_worker_protocol::FrameErrorCode::EndOfStream {
                    WorkerClientErrorCode::WorkerExited
                } else {
                    WorkerClientErrorCode::FrameReadFailed
                };
                self.stop_process();
                return Err(WorkerClientError::new(code));
            }
            Err(RecvTimeoutError::Timeout) => {
                self.stop_process();
                return Err(WorkerClientError::new(WorkerClientErrorCode::Timeout));
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.stop_process();
                return Err(WorkerClientError::new(WorkerClientErrorCode::WorkerExited));
            }
        };

        if response.request_id != request.request_id
            || response.session_id != request.session_id
            || response.sequence != request.sequence
        {
            self.stop_process();
            return Err(WorkerClientError::new(
                WorkerClientErrorCode::CorrelationMismatch,
            ));
        }
        if let ResponsePayload::Error {
            code,
            unsupported_capabilities,
        } = response.payload
        {
            return Err(WorkerClientError::rejected(code, unsupported_capabilities));
        }
        if response.protocol_version != self.protocol_version {
            self.stop_process();
            return Err(WorkerClientError::new(
                WorkerClientErrorCode::CorrelationMismatch,
            ));
        }
        Ok(response.payload)
    }

    fn stop_process(&mut self) {
        self.writer.take();
        terminate_child(&mut self.child);
        self.stopped = true;
        self.join_reader();
    }

    fn unexpected_response<T>(&mut self) -> Result<T, WorkerClientError> {
        self.stop_process();
        Err(WorkerClientError::new(
            WorkerClientErrorCode::UnexpectedResponse,
        ))
    }

    fn join_reader(&mut self) {
        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
    }
}

impl Drop for WorkerClient {
    fn drop(&mut self) {
        if !self.stopped {
            self.stop_process();
        } else {
            self.join_reader();
        }
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if started.elapsed() < timeout => thread::sleep(PROCESS_POLL_INTERVAL),
            Ok(None) | Err(_) => return false,
        }
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
