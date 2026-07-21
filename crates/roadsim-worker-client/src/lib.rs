//! Cross-platform child-process lifecycle for RoadSim workers.
//!
//! The client uses inherited standard-I/O pipes and never opens a listener. One
//! mutable client permits one in-flight request, making response correlation and
//! ordering explicit without an async runtime.

mod workdir;

pub use workdir::{
    RunDirectory, RunDirectoryError, RunDirectoryErrorCode, RunDirectoryIdentity,
    RunDirectoryLimits, RunDirectoryManager, RunDirectoryRecord, RunState,
};

use roadsim_worker_protocol::{
    AuthToken, DataValidationErrorCode, FrameError, FrameErrorCode, MetricBatch, RequestEnvelope,
    RequestPayload, ResponseEnvelope, ResponsePayload, TerminalEvent, VisualFrameBatch,
    WORKER_PROTOCOL_VERSION, WORKER_TOKEN_ENV, WorkerDataPayload, WorkerDiagnosticCode,
    capabilities_are_valid, read_data_frame, read_frame, write_frame,
};
use std::{
    collections::VecDeque,
    error::Error,
    ffi::OsString,
    fmt,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RESPONSE_QUEUE_CAPACITY: usize = 8;
const RELIABLE_DATA_QUEUE_CAPACITY: usize = 8;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Clone)]
pub struct WorkerClientConfig {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub auth_token: AuthToken,
    pub protocol_version: u32,
    pub work_directory: Option<PathBuf>,
}

impl WorkerClientConfig {
    #[must_use]
    pub fn new(program: impl Into<PathBuf>, auth_token: AuthToken) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
            auth_token,
            protocol_version: WORKER_PROTOCOL_VERSION,
            work_directory: None,
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

    #[must_use]
    pub fn with_work_directory(mut self, work_directory: impl Into<PathBuf>) -> Self {
        self.work_directory = Some(work_directory.into());
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

#[derive(Clone, Debug, PartialEq)]
pub enum ReliableWorkerEvent {
    MetricBatch {
        session_id: u64,
        sequence: u64,
        batch: MetricBatch,
    },
    Terminal {
        session_id: u64,
        sequence: u64,
        event: TerminalEvent,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataStreamErrorCode {
    Frame(FrameErrorCode),
    Validation(DataValidationErrorCode),
    SequenceOutOfOrder,
}

impl DataStreamErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frame(code) => code.as_str(),
            Self::Validation(code) => code.as_str(),
            Self::SequenceOutOfOrder => "worker.data.sequence_out_of_order",
        }
    }
}

#[derive(Clone)]
struct DataStreamState {
    latest_visual: Arc<Mutex<Option<(u64, u64, VisualFrameBatch)>>>,
    dropped_visual: Arc<AtomicU64>,
    error: Arc<Mutex<Option<DataStreamErrorCode>>>,
    stop: Arc<AtomicBool>,
}

impl DataStreamState {
    fn new() -> Self {
        Self {
            latest_visual: Arc::new(Mutex::new(None)),
            dropped_visual: Arc::new(AtomicU64::new(0)),
            error: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    fn record_error(&self, error: DataStreamErrorCode) {
        let mut current = self
            .error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current.is_none() {
            *current = Some(error);
        }
    }
}

pub struct WorkerClient {
    child: Child,
    writer: Option<ChildStdin>,
    responses: Receiver<Result<ResponseEnvelope, FrameError>>,
    reliable_events: Receiver<ReliableWorkerEvent>,
    reliable_backlog: Mutex<VecDeque<ReliableWorkerEvent>>,
    data_state: DataStreamState,
    reader_thread: Option<JoinHandle<()>>,
    data_reader_thread: Option<JoinHandle<()>>,
    auth_token: AuthToken,
    protocol_version: u32,
    next_request_id: u64,
    next_sequence: u64,
    stopped: bool,
}

impl WorkerClient {
    pub fn spawn(config: WorkerClientConfig) -> Result<Self, WorkerClientError> {
        let mut command = Command::new(&config.program);
        command
            .args(&config.arguments)
            .env(WORKER_TOKEN_ENV, config.auth_token.expose_for_transport());
        if let Some(work_directory) = &config.work_directory {
            command.current_dir(work_directory);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
        let Some(mut data_reader) = child.stderr.take() else {
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
        let data_state = DataStreamState::new();
        let data_thread_state = data_state.clone();
        let (reliable_sender, reliable_events) = mpsc::sync_channel(RELIABLE_DATA_QUEUE_CAPACITY);
        let data_reader_thread = thread::spawn(move || {
            let mut last_sequence = None;
            loop {
                let envelope = match read_data_frame(&mut data_reader) {
                    Ok(envelope) => envelope,
                    Err(error) if error.code() == FrameErrorCode::EndOfStream => break,
                    Err(error) => {
                        data_thread_state.record_error(DataStreamErrorCode::Frame(error.code()));
                        break;
                    }
                };
                if let Err(error) = envelope.validate() {
                    data_thread_state.record_error(DataStreamErrorCode::Validation(error.code()));
                    break;
                }
                if last_sequence.is_some_and(|last| envelope.sequence <= last) {
                    data_thread_state.record_error(DataStreamErrorCode::SequenceOutOfOrder);
                    break;
                }
                last_sequence = Some(envelope.sequence);
                match envelope.payload {
                    WorkerDataPayload::VisualFrame(batch) => {
                        let mut latest = data_thread_state
                            .latest_visual
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        if latest
                            .replace((envelope.session_id, envelope.sequence, batch))
                            .is_some()
                        {
                            data_thread_state
                                .dropped_visual
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    WorkerDataPayload::MetricBatch(batch) => {
                        let event = ReliableWorkerEvent::MetricBatch {
                            session_id: envelope.session_id,
                            sequence: envelope.sequence,
                            batch,
                        };
                        if !send_reliable(&reliable_sender, &data_thread_state, event) {
                            break;
                        }
                    }
                    WorkerDataPayload::Terminal(event) => {
                        let event = ReliableWorkerEvent::Terminal {
                            session_id: envelope.session_id,
                            sequence: envelope.sequence,
                            event,
                        };
                        if !send_reliable(&reliable_sender, &data_thread_state, event) {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            child,
            writer: Some(writer),
            responses,
            reliable_events,
            reliable_backlog: Mutex::new(VecDeque::new()),
            data_state,
            reader_thread: Some(reader_thread),
            data_reader_thread: Some(data_reader_thread),
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
        if !self.wait_for_exit_and_drain(timeout) {
            self.stop_process();
            return Err(WorkerClientError::new(WorkerClientErrorCode::Timeout));
        }
        self.data_state.stop.store(true, Ordering::Release);
        self.stopped = true;
        self.join_reader();
        Ok(())
    }

    /// Takes the newest visual frame. Replaced intermediate frames are counted,
    /// making renderer backpressure observable rather than silent.
    #[must_use]
    pub fn take_latest_visual_frame(&self) -> Option<(u64, u64, VisualFrameBatch)> {
        self.data_state
            .latest_visual
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    #[must_use]
    pub fn dropped_visual_frames(&self) -> u64 {
        self.data_state.dropped_visual.load(Ordering::Relaxed)
    }

    pub fn try_recv_reliable_event(&self) -> Result<ReliableWorkerEvent, TryRecvError> {
        if let Some(event) = self
            .reliable_backlog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
        {
            return Ok(event);
        }
        self.reliable_events.try_recv()
    }

    pub fn recv_reliable_event_timeout(
        &self,
        timeout: Duration,
    ) -> Result<ReliableWorkerEvent, RecvTimeoutError> {
        if let Some(event) = self
            .reliable_backlog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
        {
            return Ok(event);
        }
        self.reliable_events.recv_timeout(timeout)
    }

    #[must_use]
    pub fn data_stream_error(&self) -> Option<DataStreamErrorCode> {
        *self
            .data_state
            .error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        self.data_state.stop.store(true, Ordering::Release);
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

    fn wait_for_exit_and_drain(&mut self, timeout: Duration) -> bool {
        let started = Instant::now();
        loop {
            self.drain_reliable_queue();
            let child_exited = match self.child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => return false,
            };
            let data_reader_finished = self
                .data_reader_thread
                .as_ref()
                .is_none_or(JoinHandle::is_finished);
            if child_exited && data_reader_finished {
                self.drain_reliable_queue();
                return true;
            }
            if started.elapsed() >= timeout {
                return false;
            }
            thread::sleep(PROCESS_POLL_INTERVAL);
        }
    }

    fn drain_reliable_queue(&self) {
        let mut backlog = self
            .reliable_backlog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(event) = self.reliable_events.try_recv() {
            backlog.push_back(event);
        }
    }

    fn join_reader(&mut self) {
        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
        if let Some(data_reader_thread) = self.data_reader_thread.take() {
            let _ = data_reader_thread.join();
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

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn send_reliable(
    sender: &mpsc::SyncSender<ReliableWorkerEvent>,
    state: &DataStreamState,
    mut event: ReliableWorkerEvent,
) -> bool {
    loop {
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                if state.stop.load(Ordering::Acquire) {
                    return false;
                }
                event = returned;
                thread::sleep(PROCESS_POLL_INTERVAL);
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}
