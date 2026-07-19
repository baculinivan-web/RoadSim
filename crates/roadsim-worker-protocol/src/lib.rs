//! Versioned and resource-bounded worker control messages.
//!
//! Control frames use a four-byte little-endian length followed by JSON. This
//! crate intentionally carries no high-volume agent state; E09-T08 chooses that
//! transport separately after measurement.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use std::{error::Error, fmt, io::Read, io::Write};

pub const WORKER_PROTOCOL_VERSION: u32 = 1;
pub const MAX_CONTROL_FRAME_BYTES: usize = 1_048_576;
pub const MAX_CAPABILITIES: usize = 64;
pub const MAX_CAPABILITY_ID_BYTES: usize = 128;
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
    },
    Ping,
    OpenSession,
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
        capabilities: Vec<String>,
    },
    Pong,
    SessionOpened,
    SessionCancelled,
    ShutdownAcknowledged,
    Error {
        code: WorkerDiagnosticCode,
        unsupported_capabilities: Vec<String>,
    },
}

#[must_use]
pub fn capabilities_are_valid(capabilities: &[String]) -> bool {
    capabilities.len() <= MAX_CAPABILITIES
        && capabilities.iter().all(|capability| {
            !capability.is_empty()
                && capability.len() <= MAX_CAPABILITY_ID_BYTES
                && capability.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
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
    let encoded =
        serde_json::to_vec(message).map_err(|_| FrameError(FrameErrorCode::InvalidJson))?;
    if encoded.is_empty() {
        return Err(FrameError(FrameErrorCode::EmptyFrame));
    }
    if encoded.len() > MAX_CONTROL_FRAME_BYTES {
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
    if length > MAX_CONTROL_FRAME_BYTES {
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
    fn published_control_schema_is_valid_json() {
        let schema = include_str!("../../../schemas/worker-protocol/control-v1.schema.json");
        let parsed: serde_json::Value = serde_json::from_str(schema).unwrap();
        assert_eq!(
            parsed["$id"],
            "https://roadsim.dev/schemas/worker-protocol/control-v1.schema.json"
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
}
