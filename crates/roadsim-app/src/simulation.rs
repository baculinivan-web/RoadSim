use crate::demo;
use roadsim_backend_api::{
    BackendArtifact, BackendEvent, CompileOptions, ControlCommand, ControlKind, FrameBatch,
    RunConfig, ScenarioSnapshot, SeedAlgorithmVersion, SessionState, SimulationBackend,
    SimulationSession,
};
use roadsim_backend_fake::FakeBackend;
use roadsim_compiled_network::CompiledNetwork;
use roadsim_types::{RootSeed, Sha256Digest, SimulationTick};
use std::{error::Error, fmt, sync::Arc};

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

pub struct SimulationController {
    backend: FakeBackend,
    artifact: BackendArtifact,
    network: Arc<CompiledNetwork>,
    session: Option<Box<dyn SimulationSession>>,
    state: UiSimulationState,
    frame: Option<FrameBatch>,
    tick: SimulationTick,
    next_control_id: u64,
    error_code: Option<&'static str>,
}

impl SimulationController {
    pub fn new(auto_start: bool) -> Result<Self, SimulationError> {
        let network = Arc::new(demo::compiled_network().map_err(SimulationError)?);
        let backend = FakeBackend::new();
        let scenario = ScenarioSnapshot::new(DEMO_SCENARIO_HASH, DEMO_AGENT_COUNT)
            .map_err(|error| SimulationError(error.to_string()))?;
        let artifact = pollster::block_on(async {
            backend
                .handshake(roadsim_backend_api::ClientHello::current())
                .await?;
            backend
                .compile(network.clone(), scenario, CompileOptions)
                .await
        })
        .map_err(|error| SimulationError(error.to_string()))?;
        let mut controller = Self {
            backend,
            artifact,
            network,
            session: None,
            state: UiSimulationState::Ready,
            frame: None,
            tick: SimulationTick::new(0),
            next_control_id: 1,
            error_code: None,
        };
        if auto_start {
            controller.start();
            if controller.state == UiSimulationState::Failed {
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
        self.state
    }

    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    #[must_use]
    pub const fn error_code(&self) -> Option<&'static str> {
        self.error_code
    }

    #[must_use]
    pub fn observation(&self) -> SimulationObservation {
        SimulationObservation {
            state: self.state,
            tick: self.tick,
            agent_count: self.frame.as_ref().map_or(0, |frame| frame.agents().len()),
        }
    }

    pub fn start(&mut self) {
        if matches!(
            self.state,
            UiSimulationState::Running | UiSimulationState::Paused
        ) {
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
                self.state = UiSimulationState::Running;
                self.frame = None;
                self.tick = SimulationTick::new(0);
                self.error_code = None;
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    pub fn pause(&mut self) {
        self.control(ControlKind::Pause, UiSimulationState::Paused);
    }

    pub fn resume(&mut self) {
        self.control(ControlKind::Resume, UiSimulationState::Running);
    }

    pub fn stop(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        match pollster::block_on(session.cancel()) {
            Ok(summary) => {
                self.tick = summary.final_tick();
                self.state = UiSimulationState::Cancelled;
                self.session = None;
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    pub fn advance(&mut self) {
        if self.state != UiSimulationState::Running {
            return;
        }
        let Some(session) = &mut self.session else {
            self.fail("app.simulation.session_missing");
            return;
        };
        match pollster::block_on(session.next_event()) {
            Ok(BackendEvent::StateChanged(state)) => {
                self.state = ui_state(state);
            }
            Ok(BackendEvent::Frame(frame)) => {
                self.tick = frame.tick();
                self.frame = Some(frame);
            }
            Ok(BackendEvent::Completed(summary)) => {
                self.tick = summary.final_tick();
                self.state = UiSimulationState::Completed;
                self.session = None;
            }
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    fn control(&mut self, kind: ControlKind, next_state: UiSimulationState) {
        let Some(session) = &mut self.session else {
            return;
        };
        let control_id = self.next_control_id;
        self.next_control_id = self.next_control_id.saturating_add(1);
        let command = ControlCommand::new(control_id, control_id, kind);
        match pollster::block_on(session.control(command)) {
            Ok(_) => self.state = next_state,
            Err(error) => self.fail(error.code().as_str()),
        }
    }

    fn fail(&mut self, code: &'static str) {
        self.state = UiSimulationState::Failed;
        self.error_code = Some(code);
        self.session = None;
    }
}

const fn ui_state(state: SessionState) -> UiSimulationState {
    match state {
        SessionState::Running => UiSimulationState::Running,
        SessionState::Paused => UiSimulationState::Paused,
        SessionState::Completed => UiSimulationState::Completed,
        SessionState::Cancelled => UiSimulationState::Cancelled,
        SessionState::Failed => UiSimulationState::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_runs_observable_frames_and_controls_lifecycle() {
        let mut controller = SimulationController::new(false).unwrap();
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
    fn smoke_auto_start_observes_a_frame_after_two_advances() {
        let mut controller = SimulationController::new(true).unwrap();
        controller.advance();
        controller.advance();
        let observation = controller.observation();
        assert_eq!(observation.state, UiSimulationState::Running);
        assert_eq!(observation.tick.get(), 0);
        assert_eq!(observation.agent_count, DEMO_AGENT_COUNT as usize);
    }
}
