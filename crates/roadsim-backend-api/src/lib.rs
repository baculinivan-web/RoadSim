//! Small, versioned contract between RoadSim and simulation backends.
//!
//! The contract carries RoadSim CSN IDs and batched state. It deliberately does
//! not expose SUMO/TraCI types, worker pointers, UI state, filesystem paths, or a
//! wall-clock source.

use async_trait::async_trait;
use roadsim_compiled_network::{CapabilityId, CompiledLaneId, CompiledNetwork};
use roadsim_types::{RootSeed, Sha256Digest, SimulationTick};
use serde::Serialize;
use std::{collections::BTreeSet, error::Error, fmt, sync::Arc};

pub const BACKEND_API_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BackendId(&'static str);

impl BackendId {
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientHello {
    api_version: u32,
}

impl ClientHello {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            api_version: BACKEND_API_VERSION,
        }
    }

    #[must_use]
    pub const fn with_version(api_version: u32) -> Self {
        Self { api_version }
    }

    #[must_use]
    pub const fn api_version(self) -> u32 {
        self.api_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendHello {
    backend_id: BackendId,
    api_version: u32,
    capabilities: BTreeSet<CapabilityId>,
    deterministic: bool,
}

impl BackendHello {
    #[must_use]
    pub fn new(
        backend_id: BackendId,
        capabilities: impl IntoIterator<Item = CapabilityId>,
        deterministic: bool,
    ) -> Self {
        Self {
            backend_id,
            api_version: BACKEND_API_VERSION,
            capabilities: capabilities.into_iter().collect(),
            deterministic,
        }
    }

    #[must_use]
    pub const fn backend_id(&self) -> BackendId {
        self.backend_id
    }

    #[must_use]
    pub const fn api_version(&self) -> u32 {
        self.api_version
    }

    #[must_use]
    pub const fn capabilities(&self) -> &BTreeSet<CapabilityId> {
        &self.capabilities
    }

    #[must_use]
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendErrorPhase {
    Handshake,
    Compile,
    Runtime,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendErrorCode {
    ProtocolMismatch,
    UnsupportedCapabilities,
    EmptyNetwork,
    InvalidScenario,
    ArtifactBackendMismatch,
    ArtifactNotFound,
    InvalidRunConfig,
    InvalidLifecycle,
    IdempotencyConflict,
    TerminalSession,
    InternalState,
}

impl BackendErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolMismatch => "backend.handshake.protocol_mismatch",
            Self::UnsupportedCapabilities => "backend.capability.unsupported",
            Self::EmptyNetwork => "backend.compile.network_empty",
            Self::InvalidScenario => "backend.compile.scenario_invalid",
            Self::ArtifactBackendMismatch => "backend.start.artifact_backend_mismatch",
            Self::ArtifactNotFound => "backend.start.artifact_not_found",
            Self::InvalidRunConfig => "backend.start.run_config_invalid",
            Self::InvalidLifecycle => "backend.session.lifecycle_invalid",
            Self::IdempotencyConflict => "backend.session.idempotency_conflict",
            Self::TerminalSession => "backend.session.terminal",
            Self::InternalState => "backend.internal_state",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendError {
    phase: BackendErrorPhase,
    code: BackendErrorCode,
    unsupported_capabilities: Vec<CapabilityId>,
}

impl BackendError {
    #[must_use]
    pub fn new(phase: BackendErrorPhase, code: BackendErrorCode) -> Self {
        Self {
            phase,
            code,
            unsupported_capabilities: Vec::new(),
        }
    }

    #[must_use]
    pub fn unsupported(mut capabilities: Vec<CapabilityId>) -> Self {
        capabilities.sort_unstable();
        capabilities.dedup();
        Self {
            phase: BackendErrorPhase::Unsupported,
            code: BackendErrorCode::UnsupportedCapabilities,
            unsupported_capabilities: capabilities,
        }
    }

    #[must_use]
    pub const fn phase(&self) -> BackendErrorPhase {
        self.phase
    }

    #[must_use]
    pub const fn code(&self) -> BackendErrorCode {
        self.code
    }

    #[must_use]
    pub fn unsupported_capabilities(&self) -> &[CapabilityId] {
        &self.unsupported_capabilities
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for BackendError {}

/// Minimal immutable scenario input for the first backend contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioSnapshot {
    content_hash: Sha256Digest,
    agent_count: u32,
}

impl ScenarioSnapshot {
    pub fn new(content_hash: Sha256Digest, agent_count: u32) -> Result<Self, BackendError> {
        if !(1..=10_000).contains(&agent_count) {
            return Err(BackendError::new(
                BackendErrorPhase::Compile,
                BackendErrorCode::InvalidScenario,
            ));
        }
        Ok(Self {
            content_hash,
            agent_count,
        })
    }

    #[must_use]
    pub const fn content_hash(self) -> Sha256Digest {
        self.content_hash
    }

    #[must_use]
    pub const fn agent_count(self) -> u32 {
        self.agent_count
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompileOptions;

/// Opaque backend-owned artifact identity. It contains no implementation payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendArtifact {
    backend_id: BackendId,
    network_hash: Sha256Digest,
    scenario_hash: Sha256Digest,
}

impl BackendArtifact {
    #[must_use]
    pub const fn new(
        backend_id: BackendId,
        network_hash: Sha256Digest,
        scenario_hash: Sha256Digest,
    ) -> Self {
        Self {
            backend_id,
            network_hash,
            scenario_hash,
        }
    }

    #[must_use]
    pub const fn backend_id(self) -> BackendId {
        self.backend_id
    }

    #[must_use]
    pub const fn network_hash(self) -> Sha256Digest {
        self.network_hash
    }

    #[must_use]
    pub const fn scenario_hash(self) -> Sha256Digest {
        self.scenario_hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedAlgorithmVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedPurpose {
    AgentSpawn,
}

/// Derives a deterministic substream without wall-clock or thread-order input.
#[must_use]
pub const fn derive_seed(
    algorithm: SeedAlgorithmVersion,
    root: RootSeed,
    purpose: SeedPurpose,
    entity_id: u64,
) -> u64 {
    let algorithm_tag: u64 = match algorithm {
        SeedAlgorithmVersion::V1 => 0x726f_6164_7369_6d01,
    };
    let purpose_tag: u64 = match purpose {
        SeedPurpose::AgentSpawn => 0x6167_656e_745f_7370,
    };
    splitmix64(root.get() ^ algorithm_tag ^ purpose_tag.rotate_left(17) ^ entity_id)
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunConfig {
    scenario_hash: Sha256Digest,
    duration_ticks: u64,
    step_ticks: u64,
    root_seed: RootSeed,
    seed_algorithm: SeedAlgorithmVersion,
}

impl RunConfig {
    pub fn new(
        scenario_hash: Sha256Digest,
        duration_ticks: u64,
        step_ticks: u64,
        root_seed: RootSeed,
        seed_algorithm: SeedAlgorithmVersion,
    ) -> Result<Self, BackendError> {
        if duration_ticks == 0 || step_ticks == 0 || step_ticks > duration_ticks {
            return Err(BackendError::new(
                BackendErrorPhase::Compile,
                BackendErrorCode::InvalidRunConfig,
            ));
        }
        Ok(Self {
            scenario_hash,
            duration_ticks,
            step_ticks,
            root_seed,
            seed_algorithm,
        })
    }

    #[must_use]
    pub const fn scenario_hash(self) -> Sha256Digest {
        self.scenario_hash
    }

    #[must_use]
    pub const fn duration_ticks(self) -> u64 {
        self.duration_ticks
    }

    #[must_use]
    pub const fn step_ticks(self) -> u64 {
        self.step_ticks
    }

    #[must_use]
    pub const fn root_seed(self) -> RootSeed {
        self.root_seed
    }

    #[must_use]
    pub const fn seed_algorithm(self) -> SeedAlgorithmVersion {
        self.seed_algorithm
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl SessionState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlKind {
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlCommand {
    sequence: u64,
    idempotency_key: u64,
    kind: ControlKind,
}

impl ControlCommand {
    #[must_use]
    pub const fn new(sequence: u64, idempotency_key: u64, kind: ControlKind) -> Self {
        Self {
            sequence,
            idempotency_key,
            kind,
        }
    }

    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn idempotency_key(self) -> u64 {
        self.idempotency_key
    }

    #[must_use]
    pub const fn kind(self) -> ControlKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ack {
    sequence: u64,
    duplicate: bool,
}

impl Ack {
    #[must_use]
    pub const fn new(sequence: u64, duplicate: bool) -> Self {
        Self {
            sequence,
            duplicate,
        }
    }

    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn duplicate(self) -> bool {
        self.duplicate
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AgentState {
    agent_id: u32,
    lane_id: CompiledLaneId,
    x_m: f64,
    y_m: f64,
    heading_rad: f64,
}

impl AgentState {
    #[must_use]
    pub const fn new(
        agent_id: u32,
        lane_id: CompiledLaneId,
        x_m: f64,
        y_m: f64,
        heading_rad: f64,
    ) -> Self {
        Self {
            agent_id,
            lane_id,
            x_m,
            y_m,
            heading_rad,
        }
    }

    #[must_use]
    pub const fn agent_id(self) -> u32 {
        self.agent_id
    }

    #[must_use]
    pub const fn lane_id(self) -> CompiledLaneId {
        self.lane_id
    }

    #[must_use]
    pub const fn x_m(self) -> f64 {
        self.x_m
    }

    #[must_use]
    pub const fn y_m(self) -> f64 {
        self.y_m
    }

    #[must_use]
    pub const fn heading_rad(self) -> f64 {
        self.heading_rad
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrameBatch {
    tick: SimulationTick,
    agents: Vec<AgentState>,
}

impl FrameBatch {
    #[must_use]
    pub fn new(tick: SimulationTick, agents: Vec<AgentState>) -> Self {
        Self { tick, agents }
    }

    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    #[must_use]
    pub fn agents(&self) -> &[AgentState] {
        &self.agents
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunSummary {
    terminal_state: SessionState,
    final_tick: SimulationTick,
    emitted_frames: u64,
}

impl RunSummary {
    #[must_use]
    pub const fn new(
        terminal_state: SessionState,
        final_tick: SimulationTick,
        emitted_frames: u64,
    ) -> Self {
        Self {
            terminal_state,
            final_tick,
            emitted_frames,
        }
    }

    #[must_use]
    pub const fn terminal_state(self) -> SessionState {
        self.terminal_state
    }

    #[must_use]
    pub const fn final_tick(self) -> SimulationTick {
        self.final_tick
    }

    #[must_use]
    pub const fn emitted_frames(self) -> u64 {
        self.emitted_frames
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendEvent {
    StateChanged(SessionState),
    Frame(FrameBatch),
    Completed(RunSummary),
}

#[async_trait]
pub trait SimulationSession: Send {
    fn state(&self) -> SessionState;
    async fn control(&mut self, command: ControlCommand) -> Result<Ack, BackendError>;
    async fn next_event(&mut self) -> Result<BackendEvent, BackendError>;
    async fn cancel(&mut self) -> Result<RunSummary, BackendError>;
}

#[async_trait]
pub trait SimulationBackend: Send + Sync {
    async fn handshake(&self, client: ClientHello) -> Result<BackendHello, BackendError>;
    async fn compile(
        &self,
        network: Arc<CompiledNetwork>,
        scenario: ScenarioSnapshot,
        options: CompileOptions,
    ) -> Result<BackendArtifact, BackendError>;
    async fn start(
        &self,
        artifact: BackendArtifact,
        run: RunConfig,
    ) -> Result<Box<dyn SimulationSession>, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_derivation_has_stable_known_vectors() {
        assert_eq!(
            derive_seed(
                SeedAlgorithmVersion::V1,
                RootSeed::new(0),
                SeedPurpose::AgentSpawn,
                0,
            ),
            0xa065_ff27_d9f0_2a16
        );
        assert_eq!(
            derive_seed(
                SeedAlgorithmVersion::V1,
                RootSeed::new(42),
                SeedPurpose::AgentSpawn,
                7,
            ),
            0x150f_ca99_4ad9_f181
        );
    }

    #[test]
    fn run_config_rejects_zero_or_inverted_tick_ranges() {
        let hash = Sha256Digest::from_bytes([1; 32]);
        for (duration, step) in [(0, 1), (10, 0), (10, 11)] {
            let error = RunConfig::new(
                hash,
                duration,
                step,
                RootSeed::new(1),
                SeedAlgorithmVersion::V1,
            )
            .unwrap_err();
            assert_eq!(error.code(), BackendErrorCode::InvalidRunConfig);
        }
    }
}
