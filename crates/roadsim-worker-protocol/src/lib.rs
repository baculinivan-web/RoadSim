//! Versioned and resource-bounded worker control messages.
//!
//! Control frames use a four-byte little-endian length followed by JSON. This
//! crate intentionally carries no high-volume agent state; E09-T08 chooses that
//! transport separately after measurement.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use std::{
    error::Error,
    fmt,
    io::Read,
    io::Write,
    path::{Component, Path},
};

pub const WORKER_PROTOCOL_VERSION: u32 = 3;
pub const MAX_CONTROL_FRAME_BYTES: usize = 1_048_576;
pub const MAX_DATA_FRAME_BYTES: usize = 16_777_216;
pub const MAX_CAPABILITIES: usize = 64;
pub const MAX_CAPABILITY_ID_BYTES: usize = 128;
pub const MAX_ENGINE_NAME_BYTES: usize = 128;
pub const MAX_ENGINE_VERSION_BYTES: usize = 64;
pub const MAX_ENGINE_BUILD_REVISION_BYTES: usize = 128;
pub const MAX_BUNDLE_PATH_BYTES: usize = 240;
pub const MAX_STEPS_PER_REQUEST: u32 = 1_000_000;
pub const MAX_VISUAL_AGENTS_PER_BATCH: usize = 100_000;
pub const MAX_METRIC_SAMPLES_PER_BATCH: usize = 4_096;
pub const MAX_METRIC_DEFINITION_ID_BYTES: usize = 128;
pub const MAX_METRIC_DEFINITION_VERSION_BYTES: usize = 64;
pub const WORKER_TOKEN_ENV: &str = "ROADSIM_WORKER_TOKEN";

/// A one-time 256-bit secret passed only through an inherited worker environment
/// and the first handshake frame. Debug and Display never reveal its value.
#[derive(Clone, Eq, PartialEq)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, TokenError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(TokenError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose_for_transport(&self) -> &str {
        &self.0
    }

    /// Compares every byte so an authentication rejection does not reveal a
    /// useful matching prefix through ordinary early-exit behavior.
    #[must_use]
    pub fn matches(&self, candidate: &Self) -> bool {
        self.0
            .bytes()
            .zip(candidate.0.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthToken([REDACTED])")
    }
}

impl fmt::Display for AuthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for AuthToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AuthToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenError;

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker auth token must be 64 lowercase hexadecimal characters")
    }
}

impl Error for TokenError {}

/// Exact simulation engine identity negotiated before a worker session opens.
///
/// `build_revision` identifies the source/build input behind a release version;
/// release versions alone are not sufficient for reproducible run manifests.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineIdentity {
    name: String,
    version: String,
    build_revision: String,
}

impl EngineIdentity {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        build_revision: impl Into<String>,
    ) -> Result<Self, EngineIdentityError> {
        let identity = Self {
            name: name.into(),
            version: version.into(),
            build_revision: build_revision.into(),
        };
        if identity.is_valid() {
            Ok(identity)
        } else {
            Err(EngineIdentityError)
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    #[must_use]
    pub fn build_revision(&self) -> &str {
        &self.build_revision
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        machine_identifier_is_valid(&self.name, MAX_ENGINE_NAME_BYTES)
            && version_component_is_valid(&self.version, MAX_ENGINE_VERSION_BYTES)
            && version_component_is_valid(&self.build_revision, MAX_ENGINE_BUILD_REVISION_BYTES)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineIdentityError;

impl fmt::Display for EngineIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker engine identity is invalid")
    }
}

impl Error for EngineIdentityError {}

/// Backend-neutral reference to a compiled bundle inside the worker run directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSessionConfig {
    bundle_path: String,
    root_seed: u64,
    step_length_ms: u32,
}

impl WorkerSessionConfig {
    pub fn new(
        bundle_path: impl Into<String>,
        root_seed: u64,
        step_length_ms: u32,
    ) -> Result<Self, WorkerSessionConfigError> {
        let config = Self {
            bundle_path: bundle_path.into(),
            root_seed,
            step_length_ms,
        };
        if config.is_valid() {
            Ok(config)
        } else {
            Err(WorkerSessionConfigError)
        }
    }

    #[must_use]
    pub fn bundle_path(&self) -> &str {
        &self.bundle_path
    }

    #[must_use]
    pub const fn root_seed(&self) -> u64 {
        self.root_seed
    }

    #[must_use]
    pub const fn step_length_ms(&self) -> u32 {
        self.step_length_ms
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.bundle_path.is_empty()
            && self.bundle_path.len() <= MAX_BUNDLE_PATH_BYTES
            && (1..=1_000).contains(&self.step_length_ms)
            && Path::new(&self.bundle_path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerSessionConfigError;

impl fmt::Display for WorkerSessionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker session config is invalid")
    }
}

impl Error for WorkerSessionConfigError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerDiagnosticCode {
    #[serde(rename = "worker.protocol.version_mismatch")]
    ProtocolVersionMismatch,
    #[serde(rename = "worker.protocol.authentication_failed")]
    AuthenticationFailed,
    #[serde(rename = "worker.protocol.handshake_required")]
    HandshakeRequired,
    #[serde(rename = "worker.capability.unsupported")]
    UnsupportedCapability,
    #[serde(rename = "worker.capability.manifest_invalid")]
    InvalidCapabilityManifest,
    #[serde(rename = "worker.engine.identity_mismatch")]
    EngineIdentityMismatch,
    #[serde(rename = "worker.engine.unavailable")]
    EngineUnavailable,
    #[serde(rename = "worker.engine.start_failed")]
    EngineStartFailed,
    #[serde(rename = "worker.engine.step_failed")]
    EngineStepFailed,
    #[serde(rename = "worker.engine.close_failed")]
    EngineCloseFailed,
    #[serde(rename = "worker.session.config_invalid")]
    SessionConfigInvalid,
    #[serde(rename = "worker.session.step_invalid")]
    SessionStepInvalid,
    #[serde(rename = "worker.protocol.sequence_out_of_order")]
    SequenceOutOfOrder,
    #[serde(rename = "worker.session.already_active")]
    SessionAlreadyActive,
    #[serde(rename = "worker.session.not_found")]
    SessionNotFound,
    #[serde(rename = "worker.protocol.request_invalid")]
    InvalidRequest,
}

impl WorkerDiagnosticCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolVersionMismatch => "worker.protocol.version_mismatch",
            Self::AuthenticationFailed => "worker.protocol.authentication_failed",
            Self::HandshakeRequired => "worker.protocol.handshake_required",
            Self::UnsupportedCapability => "worker.capability.unsupported",
            Self::InvalidCapabilityManifest => "worker.capability.manifest_invalid",
            Self::EngineIdentityMismatch => "worker.engine.identity_mismatch",
            Self::EngineUnavailable => "worker.engine.unavailable",
            Self::EngineStartFailed => "worker.engine.start_failed",
            Self::EngineStepFailed => "worker.engine.step_failed",
            Self::EngineCloseFailed => "worker.engine.close_failed",
            Self::SessionConfigInvalid => "worker.session.config_invalid",
            Self::SessionStepInvalid => "worker.session.step_invalid",
            Self::SequenceOutOfOrder => "worker.protocol.sequence_out_of_order",
            Self::SessionAlreadyActive => "worker.session.already_active",
            Self::SessionNotFound => "worker.session.not_found",
            Self::InvalidRequest => "worker.protocol.request_invalid",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub request_id: u64,
    pub session_id: Option<u64>,
    pub sequence: u64,
    pub payload: RequestPayload,
}

impl RequestEnvelope {
    #[must_use]
    pub const fn new(
        protocol_version: u32,
        request_id: u64,
        session_id: Option<u64>,
        sequence: u64,
        payload: RequestPayload,
    ) -> Self {
        Self {
            protocol_version,
            request_id,
            session_id,
            sequence,
            payload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "snake_case",
    tag = "type",
    content = "data"
)]
pub enum RequestPayload {
    Handshake {
        auth_token: AuthToken,
        required_capabilities: Vec<String>,
        required_engine: Option<EngineIdentity>,
    },
    Ping,
    OpenSession {
        config: WorkerSessionConfig,
    },
    StepSession {
        steps: u32,
    },
    CloseSession,
    CancelSession,
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: u64,
    pub session_id: Option<u64>,
    pub sequence: u64,
    pub payload: ResponsePayload,
}

impl ResponseEnvelope {
    #[must_use]
    pub const fn new(request: &RequestEnvelope, payload: ResponsePayload) -> Self {
        Self {
            protocol_version: WORKER_PROTOCOL_VERSION,
            request_id: request.request_id,
            session_id: request.session_id,
            sequence: request.sequence,
            payload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "snake_case",
    tag = "type",
    content = "data"
)]
pub enum ResponsePayload {
    HandshakeAccepted {
        worker_name: String,
        worker_version: String,
        engine: EngineIdentity,
        capabilities: Vec<String>,
    },
    Pong,
    SessionOpened,
    SessionStepped {
        tick: u64,
    },
    SessionClosed,
    SessionCancelled,
    ShutdownAcknowledged,
    Error {
        code: WorkerDiagnosticCode,
        unsupported_capabilities: Vec<String>,
    },
}

/// Versioned output carried on the worker's dedicated inherited data pipe.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerDataEnvelope {
    pub protocol_version: u32,
    pub session_id: u64,
    pub sequence: u64,
    pub payload: WorkerDataPayload,
}

impl WorkerDataEnvelope {
    #[must_use]
    pub const fn new(session_id: u64, sequence: u64, payload: WorkerDataPayload) -> Self {
        Self {
            protocol_version: WORKER_PROTOCOL_VERSION,
            session_id,
            sequence,
            payload,
        }
    }

    pub fn validate(&self) -> Result<(), DataValidationError> {
        if self.protocol_version != WORKER_PROTOCOL_VERSION {
            return Err(DataValidationError::new(
                DataValidationErrorCode::ProtocolVersionMismatch,
            ));
        }
        match &self.payload {
            WorkerDataPayload::VisualFrame(batch) => batch.validate(),
            WorkerDataPayload::MetricBatch(batch) => batch.validate(),
            WorkerDataPayload::Terminal(event) => event.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "snake_case",
    tag = "type",
    content = "data"
)]
pub enum WorkerDataPayload {
    VisualFrame(VisualFrameBatch),
    MetricBatch(MetricBatch),
    Terminal(TerminalEvent),
}

/// Structure-of-arrays visual state. All arrays have exactly the same length.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisualFrameBatch {
    tick: u64,
    agent_ids: Vec<u32>,
    x_m: Vec<f64>,
    y_m: Vec<f64>,
    heading_rad: Vec<f64>,
    length_m: Vec<f64>,
    width_m: Vec<f64>,
}

impl VisualFrameBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tick: u64,
        agent_ids: Vec<u32>,
        x_m: Vec<f64>,
        y_m: Vec<f64>,
        heading_rad: Vec<f64>,
        length_m: Vec<f64>,
        width_m: Vec<f64>,
    ) -> Result<Self, DataValidationError> {
        let batch = Self {
            tick,
            agent_ids,
            x_m,
            y_m,
            heading_rad,
            length_m,
            width_m,
        };
        batch.validate()?;
        Ok(batch)
    }

    pub fn validate(&self) -> Result<(), DataValidationError> {
        let count = self.agent_ids.len();
        if count > MAX_VISUAL_AGENTS_PER_BATCH
            || self.x_m.len() != count
            || self.y_m.len() != count
            || self.heading_rad.len() != count
            || self.length_m.len() != count
            || self.width_m.len() != count
        {
            return Err(DataValidationError::new(
                DataValidationErrorCode::VisualArrayLengthMismatch,
            ));
        }
        if self.agent_ids.windows(2).any(|ids| ids[0] >= ids[1]) {
            return Err(DataValidationError::new(
                DataValidationErrorCode::AgentOrderInvalid,
            ));
        }
        if self
            .x_m
            .iter()
            .chain(&self.y_m)
            .chain(&self.heading_rad)
            .any(|value| !value.is_finite())
            || self
                .length_m
                .iter()
                .chain(&self.width_m)
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(DataValidationError::new(
                DataValidationErrorCode::NonFiniteOrInvalidValue,
            ));
        }
        Ok(())
    }

    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.agent_ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.agent_ids.is_empty()
    }

    #[must_use]
    pub fn agent_ids(&self) -> &[u32] {
        &self.agent_ids
    }

    #[must_use]
    pub fn x_m(&self) -> &[f64] {
        &self.x_m
    }

    #[must_use]
    pub fn y_m(&self) -> &[f64] {
        &self.y_m
    }

    #[must_use]
    pub fn heading_rad(&self) -> &[f64] {
        &self.heading_rad
    }

    #[must_use]
    pub fn length_m(&self) -> &[f64] {
        &self.length_m
    }

    #[must_use]
    pub fn width_m(&self) -> &[f64] {
        &self.width_m
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSample {
    tick: u64,
    definition_id: String,
    definition_version: String,
    value: f64,
}

impl MetricSample {
    pub fn new(
        tick: u64,
        definition_id: impl Into<String>,
        definition_version: impl Into<String>,
        value: f64,
    ) -> Result<Self, DataValidationError> {
        let sample = Self {
            tick,
            definition_id: definition_id.into(),
            definition_version: definition_version.into(),
            value,
        };
        sample.validate()?;
        Ok(sample)
    }

    fn validate(&self) -> Result<(), DataValidationError> {
        if self.definition_id.is_empty()
            || self.definition_id.len() > MAX_METRIC_DEFINITION_ID_BYTES
            || !self.definition_id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
            || self.definition_version.is_empty()
            || self.definition_version.len() > MAX_METRIC_DEFINITION_VERSION_BYTES
            || !self.definition_version.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+')
            })
            || !self.value.is_finite()
        {
            return Err(DataValidationError::new(
                DataValidationErrorCode::NonFiniteOrInvalidValue,
            ));
        }
        Ok(())
    }

    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub fn definition_id(&self) -> &str {
        &self.definition_id
    }

    #[must_use]
    pub fn definition_version(&self) -> &str {
        &self.definition_version
    }

    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricBatch {
    samples: Vec<MetricSample>,
}

impl MetricBatch {
    pub fn new(samples: Vec<MetricSample>) -> Result<Self, DataValidationError> {
        let batch = Self { samples };
        batch.validate()?;
        Ok(batch)
    }

    pub fn validate(&self) -> Result<(), DataValidationError> {
        if self.samples.is_empty() || self.samples.len() > MAX_METRIC_SAMPLES_PER_BATCH {
            return Err(DataValidationError::new(
                DataValidationErrorCode::MetricBatchSizeInvalid,
            ));
        }
        self.samples.iter().try_for_each(MetricSample::validate)?;
        if self.samples.windows(2).any(|samples| {
            let left = (samples[0].tick, &samples[0].definition_id);
            let right = (samples[1].tick, &samples[1].definition_id);
            left >= right
        }) {
            return Err(DataValidationError::new(
                DataValidationErrorCode::MetricOrderInvalid,
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn samples(&self) -> &[MetricSample] {
        &self.samples
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalEvent {
    pub status: TerminalStatus,
    pub diagnostic: Option<WorkerDiagnosticCode>,
}

impl TerminalEvent {
    pub fn validate(&self) -> Result<(), DataValidationError> {
        let valid = match self.status {
            TerminalStatus::Failed => self.diagnostic.is_some(),
            TerminalStatus::Completed | TerminalStatus::Cancelled => self.diagnostic.is_none(),
        };
        if valid {
            Ok(())
        } else {
            Err(DataValidationError::new(
                DataValidationErrorCode::TerminalEventInvalid,
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataValidationErrorCode {
    ProtocolVersionMismatch,
    VisualArrayLengthMismatch,
    AgentOrderInvalid,
    MetricBatchSizeInvalid,
    MetricOrderInvalid,
    TerminalEventInvalid,
    NonFiniteOrInvalidValue,
}

impl DataValidationErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolVersionMismatch => "worker.data.version_mismatch",
            Self::VisualArrayLengthMismatch => "worker.data.visual_array_length_mismatch",
            Self::AgentOrderInvalid => "worker.data.agent_order_invalid",
            Self::MetricBatchSizeInvalid => "worker.data.metric_batch_size_invalid",
            Self::MetricOrderInvalid => "worker.data.metric_order_invalid",
            Self::TerminalEventInvalid => "worker.data.terminal_event_invalid",
            Self::NonFiniteOrInvalidValue => "worker.data.value_invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataValidationError(DataValidationErrorCode);

impl DataValidationError {
    const fn new(code: DataValidationErrorCode) -> Self {
        Self(code)
    }

    #[must_use]
    pub const fn code(self) -> DataValidationErrorCode {
        self.0
    }
}

impl fmt::Display for DataValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for DataValidationError {}

#[must_use]
pub fn capabilities_are_valid(capabilities: &[String]) -> bool {
    capabilities.len() <= MAX_CAPABILITIES
        && capabilities
            .iter()
            .all(|capability| machine_identifier_is_valid(capability, MAX_CAPABILITY_ID_BYTES))
}

#[must_use]
pub fn worker_version_is_valid(version: &str) -> bool {
    version_component_is_valid(version, MAX_ENGINE_VERSION_BYTES)
}

fn machine_identifier_is_valid(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn version_component_is_valid(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameErrorCode {
    EndOfStream,
    Io,
    EmptyFrame,
    FrameTooLarge,
    InvalidJson,
}

impl FrameErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EndOfStream => "worker.frame.end_of_stream",
            Self::Io => "worker.frame.io",
            Self::EmptyFrame => "worker.frame.empty",
            Self::FrameTooLarge => "worker.frame.too_large",
            Self::InvalidJson => "worker.frame.invalid_json",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameError(FrameErrorCode);

impl FrameError {
    #[must_use]
    pub const fn code(self) -> FrameErrorCode {
        self.0
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for FrameError {}

pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<(), FrameError> {
    write_bounded_frame(writer, message, MAX_CONTROL_FRAME_BYTES)
}

pub fn write_data_frame<W: Write>(
    writer: &mut W,
    message: &WorkerDataEnvelope,
) -> Result<(), FrameError> {
    message
        .validate()
        .map_err(|_| FrameError(FrameErrorCode::InvalidJson))?;
    write_bounded_frame(writer, message, MAX_DATA_FRAME_BYTES)
}

fn write_bounded_frame<W: Write, T: Serialize>(
    writer: &mut W,
    message: &T,
    maximum_bytes: usize,
) -> Result<(), FrameError> {
    let encoded =
        serde_json::to_vec(message).map_err(|_| FrameError(FrameErrorCode::InvalidJson))?;
    if encoded.is_empty() {
        return Err(FrameError(FrameErrorCode::EmptyFrame));
    }
    if encoded.len() > maximum_bytes {
        return Err(FrameError(FrameErrorCode::FrameTooLarge));
    }
    let length =
        u32::try_from(encoded.len()).map_err(|_| FrameError(FrameErrorCode::FrameTooLarge))?;
    writer
        .write_all(&length.to_le_bytes())
        .and_then(|()| writer.write_all(&encoded))
        .and_then(|()| writer.flush())
        .map_err(|_| FrameError(FrameErrorCode::Io))
}

pub fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<T, FrameError> {
    read_bounded_frame(reader, MAX_CONTROL_FRAME_BYTES)
}

pub fn read_data_frame<R: Read>(reader: &mut R) -> Result<WorkerDataEnvelope, FrameError> {
    read_bounded_frame(reader, MAX_DATA_FRAME_BYTES)
}

fn read_bounded_frame<R: Read, T: DeserializeOwned>(
    reader: &mut R,
    maximum_bytes: usize,
) -> Result<T, FrameError> {
    let mut prefix = [0_u8; 4];
    match reader.read(&mut prefix[..1]) {
        Ok(0) => return Err(FrameError(FrameErrorCode::EndOfStream)),
        Ok(_) => {}
        Err(_) => return Err(FrameError(FrameErrorCode::Io)),
    }
    reader
        .read_exact(&mut prefix[1..])
        .map_err(|_| FrameError(FrameErrorCode::Io))?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 {
        return Err(FrameError(FrameErrorCode::EmptyFrame));
    }
    if length > maximum_bytes {
        return Err(FrameError(FrameErrorCode::FrameTooLarge));
    }
    let mut encoded = vec![0_u8; length];
    reader
        .read_exact(&mut encoded)
        .map_err(|_| FrameError(FrameErrorCode::Io))?;
    serde_json::from_slice(&encoded).map_err(|_| FrameError(FrameErrorCode::InvalidJson))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn token(byte: &str) -> AuthToken {
        AuthToken::parse(byte.repeat(64)).unwrap()
    }

    #[test]
    fn framed_handshake_round_trips_with_correlation_fields() {
        let request = RequestEnvelope::new(
            WORKER_PROTOCOL_VERSION,
            7,
            None,
            3,
            RequestPayload::Handshake {
                auth_token: token("a"),
                required_capabilities: vec!["worker.stub.lifecycle".to_owned()],
                required_engine: Some(
                    EngineIdentity::new("eclipse.sumo", "1.27.1", "7717f2379d9e").unwrap(),
                ),
            },
        );
        let mut frame = Vec::new();
        write_frame(&mut frame, &request).unwrap();
        let decoded: RequestEnvelope = read_frame(&mut Cursor::new(frame)).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn oversized_length_is_rejected_before_payload_allocation() {
        let prefix = ((MAX_CONTROL_FRAME_BYTES + 1) as u32).to_le_bytes();
        let error = read_frame::<_, RequestEnvelope>(&mut Cursor::new(prefix)).unwrap_err();
        assert_eq!(error.code(), FrameErrorCode::FrameTooLarge);
    }

    #[test]
    fn malformed_and_truncated_frames_have_stable_codes() {
        let error =
            read_frame::<_, RequestEnvelope>(&mut Cursor::new(Vec::<u8>::new())).unwrap_err();
        assert_eq!(error.code(), FrameErrorCode::EndOfStream);

        let mut invalid = 1_u32.to_le_bytes().to_vec();
        invalid.push(b'{');
        let error = read_frame::<_, RequestEnvelope>(&mut Cursor::new(invalid)).unwrap_err();
        assert_eq!(error.code(), FrameErrorCode::InvalidJson);
    }

    #[test]
    fn token_is_validated_compared_and_redacted() {
        let first = token("a");
        let same = token("a");
        let other = token("b");
        assert!(first.matches(&same));
        assert!(!first.matches(&other));
        assert_eq!(format!("{first:?}"), "AuthToken([REDACTED])");
        assert_eq!(first.to_string(), "[REDACTED]");
        assert!(AuthToken::parse("A".repeat(64)).is_err());
        assert!(AuthToken::parse("a".repeat(63)).is_err());
    }

    #[test]
    fn capability_ids_are_bounded_and_machine_safe() {
        assert!(capabilities_are_valid(
            &["worker.stub.lifecycle".to_owned()]
        ));
        assert!(!capabilities_are_valid(&["SUMO unsafe".to_owned()]));
        assert!(!capabilities_are_valid(&vec![
            "a".to_owned();
            MAX_CAPABILITIES + 1
        ]));
    }

    #[test]
    fn engine_identity_is_exact_bounded_and_machine_safe() {
        let identity = EngineIdentity::new(
            "eclipse.sumo",
            "1.27.1",
            "7717f2379d9e314a0c81c5cec748444de06a2a91",
        )
        .unwrap();
        assert_eq!(identity.name(), "eclipse.sumo");
        assert_eq!(identity.version(), "1.27.1");
        assert_eq!(
            identity.build_revision(),
            "7717f2379d9e314a0c81c5cec748444de06a2a91"
        );
        assert!(EngineIdentity::new("SUMO unsafe", "1.27.1", "7717f23").is_err());
        assert!(EngineIdentity::new("eclipse.sumo", "", "7717f23").is_err());
        assert!(EngineIdentity::new("eclipse.sumo", "1.27.1", "bad revision").is_err());
    }

    #[test]
    fn session_config_accepts_only_relative_bounded_bundle_paths() {
        let config = WorkerSessionConfig::new("bundle/run.sumocfg", 123, 100).unwrap();
        assert_eq!(config.bundle_path(), "bundle/run.sumocfg");
        assert_eq!(config.root_seed(), 123);
        assert_eq!(config.step_length_ms(), 100);
        assert!(WorkerSessionConfig::new("../outside.sumocfg", 1, 100).is_err());
        assert!(WorkerSessionConfig::new("/outside.sumocfg", 1, 100).is_err());
        assert!(WorkerSessionConfig::new("bundle/run.sumocfg", 1, 0).is_err());
        assert!(WorkerSessionConfig::new("bundle/run.sumocfg", 1, 1_001).is_err());
    }

    #[test]
    fn published_control_schema_is_valid_json() {
        let schema = include_str!("../../../schemas/worker-protocol/control-v3.schema.json");
        let parsed: serde_json::Value = serde_json::from_str(schema).unwrap();
        assert_eq!(
            parsed["$id"],
            "https://roadsim.dev/schemas/worker-protocol/control-v3.schema.json"
        );
        let data_schema = include_str!("../../../schemas/worker-protocol/data-v3.schema.json");
        let parsed: serde_json::Value = serde_json::from_str(data_schema).unwrap();
        assert_eq!(
            parsed["$id"],
            "https://roadsim.dev/schemas/worker-protocol/data-v3.schema.json"
        );
    }

    #[test]
    fn unknown_envelope_fields_fail_closed() {
        let encoded = br#"{
            "protocol_version":1,
            "request_id":1,
            "session_id":null,
            "sequence":1,
            "payload":{"type":"ping"},
            "unexpected":true
        }"#;
        let mut frame = (encoded.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(encoded);
        let error = read_frame::<_, RequestEnvelope>(&mut Cursor::new(frame)).unwrap_err();
        assert_eq!(error.code(), FrameErrorCode::InvalidJson);
    }

    #[test]
    fn diagnostic_wire_value_is_the_stable_public_code() {
        let encoded = serde_json::to_string(&WorkerDiagnosticCode::UnsupportedCapability).unwrap();
        assert_eq!(encoded, r#""worker.capability.unsupported""#);
    }

    #[test]
    fn data_frame_round_trips_soa_visual_batch() {
        let batch = VisualFrameBatch::new(
            10,
            vec![1, 2],
            vec![3.0, 4.0],
            vec![5.0, 6.0],
            vec![0.0, 1.0],
            vec![4.5, 4.5],
            vec![1.8, 1.8],
        )
        .unwrap();
        let envelope = WorkerDataEnvelope::new(7, 11, WorkerDataPayload::VisualFrame(batch));
        let mut encoded = Vec::new();
        write_data_frame(&mut encoded, &envelope).unwrap();
        let decoded = read_data_frame(&mut Cursor::new(encoded)).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn data_batches_reject_misaligned_arrays_and_invalid_metrics() {
        let error = VisualFrameBatch::new(
            0,
            vec![1, 2],
            vec![0.0],
            vec![0.0, 0.0],
            vec![0.0, 0.0],
            vec![4.5, 4.5],
            vec![1.8, 1.8],
        )
        .unwrap_err();
        assert_eq!(
            error.code(),
            DataValidationErrorCode::VisualArrayLengthMismatch
        );
        assert!(MetricSample::new(0, "unsafe metric id", "1.0.0", 1.0).is_err());
        assert!(MetricSample::new(0, "metric.safe", "1.0.0", f64::NAN).is_err());
        assert!(MetricSample::new(0, "metric.safe", "", 1.0).is_err());
        assert!(MetricBatch::new(Vec::new()).is_err());
        assert!(
            TerminalEvent {
                status: TerminalStatus::Failed,
                diagnostic: None,
            }
            .validate()
            .is_err()
        );

        let duplicate_agents = VisualFrameBatch::new(
            0,
            vec![1, 1],
            vec![0.0, 1.0],
            vec![0.0, 0.0],
            vec![0.0, 0.0],
            vec![4.5, 4.5],
            vec![1.8, 1.8],
        )
        .unwrap_err();
        assert_eq!(
            duplicate_agents.code(),
            DataValidationErrorCode::AgentOrderInvalid
        );
    }
}
