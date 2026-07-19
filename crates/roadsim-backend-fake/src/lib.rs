//! Deterministic in-memory backend used to exercise the RoadSim contract and UI.

use async_trait::async_trait;
use roadsim_backend_api::{
    Ack, AgentFootprint, AgentState, BACKEND_API_VERSION, BackendArtifact, BackendError,
    BackendErrorCode, BackendErrorPhase, BackendEvent, BackendHello, BackendId, ClientHello,
    CompileOptions, ControlCommand, ControlKind, FrameBatch, RunConfig, RunSummary,
    ScenarioSnapshot, SeedPurpose, SessionState, SimulationBackend, SimulationSession, derive_seed,
};
use roadsim_compiled_network::{CapabilityId, CompiledNetwork};
use roadsim_types::{Sha256Digest, SimulationTick};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const FAKE_BACKEND_ID: BackendId = BackendId::new("roadsim.fake.v1");
const MAX_REMEMBERED_CONTROL_KEYS: usize = 64;
const DEMO_CAR_LENGTH_M: f64 = 4.5;
const DEMO_CAR_WIDTH_M: f64 = 1.8;

#[derive(Clone)]
struct StoredArtifact {
    network: Arc<CompiledNetwork>,
    scenario: ScenarioSnapshot,
}

#[derive(Default)]
pub struct FakeBackend {
    artifacts: Mutex<BTreeMap<(Sha256Digest, Sha256Digest), StoredArtifact>>,
}

impl FakeBackend {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            artifacts: Mutex::new(BTreeMap::new()),
        }
    }

    fn capabilities() -> [CapabilityId; 1] {
        [CapabilityId::RoadVehiclesBasic]
    }
}

#[async_trait]
impl SimulationBackend for FakeBackend {
    async fn handshake(&self, client: ClientHello) -> Result<BackendHello, BackendError> {
        if client.api_version() != BACKEND_API_VERSION {
            return Err(BackendError::new(
                BackendErrorPhase::Handshake,
                BackendErrorCode::ProtocolMismatch,
            ));
        }
        Ok(BackendHello::new(
            FAKE_BACKEND_ID,
            Self::capabilities(),
            true,
        ))
    }

    async fn compile(
        &self,
        network: Arc<CompiledNetwork>,
        scenario: ScenarioSnapshot,
        _options: CompileOptions,
    ) -> Result<BackendArtifact, BackendError> {
        if network.lanes().is_empty() {
            return Err(BackendError::new(
                BackendErrorPhase::Compile,
                BackendErrorCode::EmptyNetwork,
            ));
        }
        let supported = Self::capabilities().into_iter().collect::<Vec<_>>();
        let unsupported = network
            .requirements()
            .values()
            .iter()
            .filter(|capability| !supported.contains(capability))
            .copied()
            .collect::<Vec<_>>();
        if !unsupported.is_empty() {
            return Err(BackendError::unsupported(unsupported));
        }

        let network_hash = network.header().content_hash();
        let scenario_hash = scenario.content_hash();
        self.artifacts
            .lock()
            .map_err(|_| {
                BackendError::new(BackendErrorPhase::Compile, BackendErrorCode::InternalState)
            })?
            .insert(
                (network_hash, scenario_hash),
                StoredArtifact { network, scenario },
            );
        Ok(BackendArtifact::new(
            FAKE_BACKEND_ID,
            network_hash,
            scenario_hash,
        ))
    }

    async fn start(
        &self,
        artifact: BackendArtifact,
        run: RunConfig,
    ) -> Result<Box<dyn SimulationSession>, BackendError> {
        if artifact.backend_id() != FAKE_BACKEND_ID {
            return Err(BackendError::new(
                BackendErrorPhase::Compile,
                BackendErrorCode::ArtifactBackendMismatch,
            ));
        }
        if artifact.scenario_hash() != run.scenario_hash() {
            return Err(BackendError::new(
                BackendErrorPhase::Compile,
                BackendErrorCode::InvalidRunConfig,
            ));
        }
        let stored = self
            .artifacts
            .lock()
            .map_err(|_| {
                BackendError::new(BackendErrorPhase::Compile, BackendErrorCode::InternalState)
            })?
            .get(&(artifact.network_hash(), artifact.scenario_hash()))
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    BackendErrorPhase::Compile,
                    BackendErrorCode::ArtifactNotFound,
                )
            })?;
        Ok(Box::new(FakeSession::new(stored, run)))
    }
}

struct FakeSession {
    artifact: StoredArtifact,
    run: RunConfig,
    state: SessionState,
    next_tick: u64,
    emitted_frames: u64,
    pending_state_event: bool,
    idempotency: BTreeMap<u64, ControlKind>,
}

impl FakeSession {
    fn new(artifact: StoredArtifact, run: RunConfig) -> Self {
        Self {
            artifact,
            run,
            state: SessionState::Running,
            next_tick: 0,
            emitted_frames: 0,
            pending_state_event: true,
            idempotency: BTreeMap::new(),
        }
    }

    fn terminal_error() -> BackendError {
        BackendError::new(
            BackendErrorPhase::Runtime,
            BackendErrorCode::TerminalSession,
        )
    }

    fn frame(&self, tick: u64) -> Result<FrameBatch, BackendError> {
        let lane_count = self.artifact.network.lanes().len();
        let footprint = AgentFootprint::new(DEMO_CAR_LENGTH_M, DEMO_CAR_WIDTH_M)?;
        let agents = (0..self.artifact.scenario.agent_count())
            .map(|agent_id| {
                let lane_index = agent_id as usize % lane_count;
                let lane_id = roadsim_compiled_network::CompiledLaneId::new(lane_index as u32);
                let lane = self
                    .artifact
                    .network
                    .lanes()
                    .lane(lane_id)
                    .expect("agent lane index is bounded by the immutable LaneTable");
                let phase = derive_seed(
                    self.run.seed_algorithm(),
                    self.run.root_seed(),
                    SeedPurpose::AgentSpawn,
                    u64::from(agent_id),
                ) as f64
                    / u64::MAX as f64;
                let progress = (phase + tick as f64 / lane.length_m()).fract();
                let x_m = lane.start().x_m() + (lane.end().x_m() - lane.start().x_m()) * progress;
                let y_m = lane.start().y_m() + (lane.end().y_m() - lane.start().y_m()) * progress;
                let heading_rad = libm::atan2(
                    lane.end().y_m() - lane.start().y_m(),
                    lane.end().x_m() - lane.start().x_m(),
                );
                AgentState::new(agent_id, lane_id, x_m, y_m, heading_rad, footprint)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FrameBatch::new(SimulationTick::new(tick), agents))
    }
}

#[async_trait]
impl SimulationSession for FakeSession {
    fn state(&self) -> SessionState {
        self.state
    }

    async fn control(&mut self, command: ControlCommand) -> Result<Ack, BackendError> {
        if self.state.is_terminal() {
            return Err(Self::terminal_error());
        }
        if let Some(previous) = self.idempotency.get(&command.idempotency_key()) {
            if *previous == command.kind() {
                return Ok(Ack::new(command.sequence(), true));
            }
            return Err(BackendError::new(
                BackendErrorPhase::Runtime,
                BackendErrorCode::IdempotencyConflict,
            ));
        }
        match (self.state, command.kind()) {
            (SessionState::Running, ControlKind::Pause) => self.state = SessionState::Paused,
            (SessionState::Paused, ControlKind::Resume) => self.state = SessionState::Running,
            _ => {
                return Err(BackendError::new(
                    BackendErrorPhase::Runtime,
                    BackendErrorCode::InvalidLifecycle,
                ));
            }
        }
        if self.idempotency.len() >= MAX_REMEMBERED_CONTROL_KEYS {
            self.idempotency.pop_first();
        }
        self.idempotency
            .insert(command.idempotency_key(), command.kind());
        self.pending_state_event = true;
        Ok(Ack::new(command.sequence(), false))
    }

    async fn next_event(&mut self) -> Result<BackendEvent, BackendError> {
        if self.state.is_terminal() {
            return Err(Self::terminal_error());
        }
        if self.pending_state_event {
            self.pending_state_event = false;
            return Ok(BackendEvent::StateChanged(self.state));
        }
        if self.state == SessionState::Paused {
            return Err(BackendError::new(
                BackendErrorPhase::Runtime,
                BackendErrorCode::InvalidLifecycle,
            ));
        }
        if self.next_tick >= self.run.duration_ticks() {
            self.state = SessionState::Completed;
            return Ok(BackendEvent::Completed(RunSummary::new(
                self.state,
                SimulationTick::new(self.run.duration_ticks()),
                self.emitted_frames,
            )));
        }

        let tick = self.next_tick;
        self.next_tick = self
            .next_tick
            .checked_add(self.run.step_ticks())
            .unwrap_or(u64::MAX)
            .min(self.run.duration_ticks());
        self.emitted_frames += 1;
        self.frame(tick).map(BackendEvent::Frame)
    }

    async fn cancel(&mut self) -> Result<RunSummary, BackendError> {
        if self.state.is_terminal() {
            return Err(Self::terminal_error());
        }
        self.state = SessionState::Cancelled;
        Ok(RunSummary::new(
            self.state,
            SimulationTick::new(self.next_tick.min(self.run.duration_ticks())),
            self.emitted_frames,
        ))
    }
}
