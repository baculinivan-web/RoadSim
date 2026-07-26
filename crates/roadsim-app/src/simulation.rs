use roadsim_application::{RunOrchestrator, RunOutcome, RunRequest, RunState};
use roadsim_backend_api::{
    BackendArtifact, BackendEvent, CompileOptions, ControlCommand, ControlKind, FrameBatch,
    RunConfig, ScenarioSnapshot, SeedAlgorithmVersion, SessionState, SimulationBackend,
    SimulationSession,
};
use roadsim_backend_fake::FakeBackend;
use roadsim_backend_sumo::{SumoRoadExportOptions, SumoVehicleTypeOptions};
use roadsim_backend_sumo_client::{NetworkMaterialization, SumoRunnerBackend, SumoRunnerConfig};
use roadsim_compiled_network::{CompiledDemandTable, CompiledNetwork};
use roadsim_types::{RootSeed, Sha256Digest, SimulationTick};
use roadsim_worker_protocol::{AuthToken, EngineIdentity};
use std::hash::{BuildHasher, Hasher};
use std::{error::Error, fmt, path::PathBuf, sync::Arc};

const DEMO_AGENT_COUNT: u32 = 18;
const DEMO_DURATION_TICKS: u64 = 1_200;
const DEMO_STEP_TICKS: u64 = 1;
const DEMO_ROOT_SEED: u64 = 20_260_718;
const DEMO_SCENARIO_HASH: Sha256Digest = Sha256Digest::from_bytes([0x42; 32]);

#[derive(Debug)]
pub struct SimulationError(String);

impl fmt::Display for SimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SimulationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSimulationState {
    Ready,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl UiSimulationState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Готова",
            Self::Running => "Выполняется",
            Self::Paused => "Пауза",
            Self::Completed => "Завершена",
            Self::Cancelled => "Остановлена",
            Self::Failed => "Ошибка",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulationObservation {
    pub state: UiSimulationState,
    pub tick: SimulationTick,
    pub agent_count: usize,
}

/// Which engine drives the run, chosen once at startup from the environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendChoice {
    Fake,
    Sumo,
}

impl BackendChoice {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fake => "roadsim.fake.v1 · deterministic",
            Self::Sumo => "roadsim.sumo.v1 · worker",
        }
    }
}

/// Builds the SUMO backend when the environment opts into it.
///
/// `ROADSIM_SUMO_WORKER` and `ROADSIM_NETCONVERT` select the worker binary and
/// the exact netconvert; `ROADSIM_SUMO_BRIDGE` adds the native bridge library.
/// Missing netconvert with a configured worker is a configuration error, not a
/// silent fallback to the fake backend.
fn backend_from_environment() -> Result<(Box<dyn SimulationBackend>, BackendChoice), SimulationError>
{
    let Some(worker) = std::env::var_os("ROADSIM_SUMO_WORKER") else {
        return Ok((Box::new(FakeBackend::new()), BackendChoice::Fake));
    };
    let Some(netconvert) = std::env::var_os("ROADSIM_NETCONVERT") else {
        return Err(SimulationError("app.backend.netconvert_missing".to_owned()));
    };
    // Random one-run token: authentication material, not model behavior.
    let entropy = std::collections::hash_map::RandomState::new();
    let mut token = String::new();
    while token.len() < 64 {
        let mut hasher = entropy.build_hasher();
        hasher.write_usize(token.len());
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token.truncate(64);
    let run_root = std::env::var_os("ROADSIM_RUN_ROOT")
        .map_or_else(|| std::env::temp_dir().join("roadsim-runs"), PathBuf::from);
    std::fs::create_dir_all(&run_root)
        .map_err(|error| SimulationError(format!("app.backend.run_root: {error}")))?;
    let mut config = SumoRunnerConfig::new(
        worker,
        NetworkMaterialization::Netconvert(PathBuf::from(netconvert)),
        run_root,
        AuthToken::parse(token).map_err(|_| SimulationError("app.backend.token".to_owned()))?,
        SumoRoadExportOptions::new(13.89).map_err(|error| SimulationError(error.to_string()))?,
        SumoVehicleTypeOptions::new(4.5, 1.8, 13.89)
            .map_err(|error| SimulationError(error.to_string()))?,
    )
    .with_engine(
        EngineIdentity::new(
            "eclipse.sumo",
            "1.27.1",
            "7717f2379d9e314a0c81c5cec748444de06a2a91",
        )
        .map_err(|error| SimulationError(error.to_string()))?,
    );
    if let Some(bridge) = std::env::var_os("ROADSIM_SUMO_BRIDGE") {
        config = config
            .with_worker_argument("--bridge")
            .with_worker_argument(bridge);
    }
    Ok((
        Box::new(SumoRunnerBackend::new(config)),
        BackendChoice::Sumo,
    ))
}

pub struct SimulationController {
    backend: Box<dyn SimulationBackend>,
    backend_choice: BackendChoice,
    demand: Option<Arc<CompiledDemandTable>>,
    artifact: BackendArtifact,
    network: Arc<CompiledNetwork>,
    session: Option<Box<dyn SimulationSession>>,
    /// Single source of truth for which lifecycle actions are legal.
    orchestrator: RunOrchestrator,
    frame: Option<FrameBatch>,
    next_control_id: u64,
    error_code: Option<&'static str>,
}

impl SimulationController {
    pub fn new(network: Arc<CompiledNetwork>, auto_start: bool) -> Result<Self, SimulationError> {
        Self::with_demand(network, None, auto_start)
    }

    pub fn with_demand(
        network: Arc<CompiledNetwork>,
        demand: Option<Arc<CompiledDemandTable>>,
        auto_start: bool,
    ) -> Result<Self, SimulationError> {
        let (backend, backend_choice) = backend_from_environment()?;
        let scenario = ScenarioSnapshot::new(DEMO_SCENARIO_HASH, DEMO_AGENT_COUNT)
            .map_err(|error| SimulationError(error.to_string()))?;
        let mut options = CompileOptions::none();
        if let Some(demand) = &demand {
            options = options.with_demand(demand.clone());
        }
        let artifact = pollster::block_on(async {
            backend
                .handshake(roadsim_backend_api::ClientHello::current())
                .await?;
            backend.compile(network.clone(), scenario, options).await
        })
        .map_err(|error| SimulationError(error.to_string()))?;
        let mut orchestrator = RunOrchestrator::new();
        // The artifact above is the compile step the orchestrator asks for.
        orchestrator
            .request(RunRequest::Prepare)
            .map_err(|error| SimulationError(error.code().as_str().to_owned()))?;
        let mut controller = Self {
            backend,
            backend_choice,
            demand,
            artifact,
            network,
            session: None,
            orchestrator,
            frame: None,
            next_control_id: 1,
            error_code: None,
        };
        if auto_start {
            controller.start();
            if controller.state() == UiSimulationState::Failed {
                return Err(SimulationError(
                    controller
                        .error_code
                        .unwrap_or("backend.start.failed")
                        .to_owned(),
                ));
            }
        }
        Ok(controller)
    }

    /// Swaps in a newly compiled network while no run is active.
    ///
    /// The old artifact and history are released: a new artifact means a new
    /// run lifecycle, so the orchestrator restarts from `Prepared`.
    pub fn replace_network(
        &mut self,
        network: Arc<CompiledNetwork>,
        demand: Option<Arc<CompiledDemandTable>>,
    ) -> Result<(), SimulationError> {
        if matches!(
            self.state(),
            UiSimulationState::Running | UiSimulationState::Paused
        ) {
            return Err(SimulationError("app.simulation.run_active".to_owned()));
        }
        let scenario = ScenarioSnapshot::new(DEMO_SCENARIO_HASH, DEMO_AGENT_COUNT)
            .map_err(|error| SimulationError(error.to_string()))?;
        let mut options = CompileOptions::none();
        if let Some(demand) = &demand {
            options = options.with_demand(demand.clone());
        }
        let artifact = pollster::block_on(self.backend.compile(network.clone(), scenario, options))
            .map_err(|error| SimulationError(error.to_string()))?;
        self.demand = demand;
        let mut orchestrator = RunOrchestrator::new();
        orchestrator
            .request(RunRequest::Prepare)
            .map_err(|error| SimulationError(error.code().as_str().to_owned()))?;
        self.artifact = artifact;
        self.network = network;
        self.orchestrator = orchestrator;
        self.session = None;
        self.frame = None;
        self.error_code = None;
        Ok(())
    }

    #[must_use]
    pub const fn network(&self) -> &Arc<CompiledNetwork> {
        &self.network
    }

    #[must_use]
    pub const fn frame(&self) -> Option<&FrameBatch> {
        self.frame.as_ref()
    }

    #[must_use]
    pub const fn state(&self) -> UiSimulationState {
        ui_state_for(self.orchestrator.state())
    }

    #[must_use]
    pub const fn backend_choice(&self) -> BackendChoice {
        self.backend_choice
    }

    /// Whether the UI may offer this action; invalid actions stay disabled.
    #[must_use]
    pub const fn accepts(&self, request: RunRequest) -> bool {
        self.orchestrator.accepts(request)
    }

    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.orchestrator.tick()
    }

    #[must_use]
    pub const fn error_code(&self) -> Option<&'static str> {
        self.error_code
    }

    #[must_use]
    pub fn observation(&self) -> SimulationObservation {
        SimulationObservation {
            state: self.state(),
            tick: self.tick(),
            agent_count: self.frame.as_ref().map_or(0, |frame| frame.agents().len()),
        }
    }

    pub fn start(&mut self) {
        if self.orchestrator.state().is_terminal() && self.orchestrator.accepts(RunRequest::Reset) {
            // A finished run is released before the same artifact runs again.
            self.request_or_report(RunRequest::Reset);
            self.session = None;
        }
        if !self.request_or_report(RunRequest::Start) {
            return;
        }
        let run = match RunConfig::new(
            DEMO_SCENARIO_HASH,
            DEMO_DURATION_TICKS,
            DEMO_STEP_TICKS,
            RootSeed::new(DEMO_ROOT_SEED),
            SeedAlgorithmVersion::V1,
        ) {
            Ok(run) => run,
            Err(error) => {
                self.fail(error.code().as_str());
                return;
            }
        };
        match pollster::block_on(self.backend.start(self.artifact, run)) {
            Ok(session) => {
                self.session = Some(session);
                self.frame = None;
                self.error_code = None;
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    pub fn pause(&mut self) {
        if self.request_or_report(RunRequest::Pause) {
            self.control(ControlKind::Pause);
        }
    }

    pub fn resume(&mut self) {
        if self.request_or_report(RunRequest::Resume) {
            self.control(ControlKind::Resume);
        }
    }

    pub fn stop(&mut self) {
        if !self.request_or_report(RunRequest::Cancel) {
            return;
        }
        let Some(session) = &mut self.session else {
            self.fail("app.simulation.session_missing");
            return;
        };
        match pollster::block_on(session.cancel()) {
            Ok(summary) => {
                let tick = summary.final_tick();
                let _ = self.orchestrator.observe_tick(tick);
                self.finish(RunOutcome::Cancelled);
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    pub fn advance(&mut self) {
        if self.state() != UiSimulationState::Running {
            return;
        }
        let Some(session) = &mut self.session else {
            self.fail("app.simulation.session_missing");
            return;
        };
        match pollster::block_on(session.next_event()) {
            Ok(BackendEvent::StateChanged(state)) => self.observe_session_state(state),
            Ok(BackendEvent::Frame(frame)) => {
                let _ = self.orchestrator.observe_tick(frame.tick());
                self.frame = Some(frame);
            }
            Ok(BackendEvent::Completed(summary)) => {
                let tick = summary.final_tick();
                let _ = self.orchestrator.observe_tick(tick);
                self.finish(RunOutcome::Completed);
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    /// Applies a lifecycle request, reporting a refusal as a stable code.
    fn request_or_report(&mut self, request: RunRequest) -> bool {
        match self.orchestrator.request(request) {
            Ok(_) => {
                self.error_code = None;
                true
            }
            Err(error) => {
                self.error_code = Some(error.code().as_str());
                false
            }
        }
    }

    fn observe_session_state(&mut self, state: SessionState) {
        if self.orchestrator.observe_session_state(state).is_err() {
            self.fail("app.simulation.session_missing");
            return;
        }
        if self.orchestrator.state().is_terminal() {
            self.session = None;
        }
    }

    fn finish(&mut self, outcome: RunOutcome) {
        let _ = self.orchestrator.finish(outcome);
        self.session = None;
    }

    fn control(&mut self, kind: ControlKind) {
        let Some(session) = &mut self.session else {
            self.fail("app.simulation.session_missing");
            return;
        };
        let control_id = self.next_control_id;
        self.next_control_id = self.next_control_id.saturating_add(1);
        let command = ControlCommand::new(control_id, control_id, kind);
        if let Err(error) = pollster::block_on(session.control(command)) {
            self.fail(error.code().as_str());
        }
    }

    fn fail(&mut self, code: &'static str) {
        let _ = self.orchestrator.finish(RunOutcome::Failed);
        self.error_code = Some(code);
        self.session = None;
    }
}

/// Projects the orchestrated run state onto the UI vocabulary.
const fn ui_state_for(state: RunState) -> UiSimulationState {
    match state {
        RunState::Idle | RunState::Prepared => UiSimulationState::Ready,
        RunState::Running => UiSimulationState::Running,
        RunState::Paused => UiSimulationState::Paused,
        RunState::Completed => UiSimulationState::Completed,
        RunState::Cancelled => UiSimulationState::Cancelled,
        RunState::Failed => UiSimulationState::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_runs_observable_frames_and_controls_lifecycle() {
        let network = std::sync::Arc::new(crate::demo::compiled_network().unwrap());
        let mut controller = SimulationController::new(network, false).unwrap();
        assert_eq!(controller.state(), UiSimulationState::Ready);
        controller.start();
        controller.advance();
        controller.advance();
        assert_eq!(controller.state(), UiSimulationState::Running);
        assert_eq!(
            controller.frame().unwrap().agents().len(),
            DEMO_AGENT_COUNT as usize
        );
        controller.pause();
        assert_eq!(controller.state(), UiSimulationState::Paused);
        controller.resume();
        assert_eq!(controller.state(), UiSimulationState::Running);
        controller.stop();
        assert_eq!(controller.state(), UiSimulationState::Cancelled);
        controller.start();
        controller.advance();
        controller.advance();
        assert_eq!(controller.state(), UiSimulationState::Running);
        assert_eq!(
            controller.frame().unwrap().agents().len(),
            DEMO_AGENT_COUNT as usize
        );
    }

    #[test]
    fn replace_network_is_refused_while_a_run_is_active() {
        let network = std::sync::Arc::new(crate::demo::compiled_network().unwrap());
        let mut controller = SimulationController::new(network.clone(), false).unwrap();
        controller.start();
        assert!(controller.replace_network(network.clone(), None).is_err());
        controller.stop();
        controller.replace_network(network, None).unwrap();
        assert_eq!(controller.state(), UiSimulationState::Ready);
        controller.start();
        controller.advance();
        controller.advance();
        assert_eq!(controller.state(), UiSimulationState::Running);
    }

    #[test]
    fn smoke_auto_start_observes_a_frame_after_two_advances() {
        let network = std::sync::Arc::new(crate::demo::compiled_network().unwrap());
        let mut controller = SimulationController::new(network, true).unwrap();
        controller.advance();
        controller.advance();
        let observation = controller.observation();
        assert_eq!(observation.state, UiSimulationState::Running);
        assert_eq!(observation.tick.get(), 0);
        assert_eq!(observation.agent_count, DEMO_AGENT_COUNT as usize);
    }
}
