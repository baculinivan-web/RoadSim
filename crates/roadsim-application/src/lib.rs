//! Backend-independent run orchestration for the RoadSim application layer.
//!
//! The orchestrator owns the observable lifecycle of one simulation run:
//! which transitions a user or UI may request, which are refused, and how a
//! run reaches exactly one terminal outcome. It deliberately performs no I/O
//! and drives no backend itself — the caller applies the returned intent — so
//! the same state machine is testable without an engine, a worker or a GPU.

use roadsim_backend_api::SessionState;
use roadsim_types::SimulationTick;
use std::{error::Error, fmt};

/// Observable state of one orchestrated run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunState {
    /// No run exists yet, or the previous one was cleared for a restart.
    Idle,
    /// A compiled artifact exists and a run may be started.
    Prepared,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl RunState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Stable identifier for diagnostics, logs and UI message keys.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Prepared => "prepared",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// Lifecycle request a user or UI can make of a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunRequest {
    Prepare,
    Start,
    Pause,
    Resume,
    Cancel,
    /// Clears a terminal run so the same artifact can be started again.
    Reset,
}

impl RunRequest {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Start => "start",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::Reset => "reset",
        }
    }
}

/// Side effect the caller must perform after an accepted request.
///
/// The orchestrator never calls a backend: returning the intent keeps the
/// state machine synchronous and testable while the caller owns async I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunIntent {
    /// Compile the current model into a backend artifact.
    Compile,
    /// Start a session from the prepared artifact.
    StartSession,
    /// Send the matching control command to the running session.
    PauseSession,
    ResumeSession,
    CancelSession,
    /// Drop any session state; nothing has to be sent to the backend.
    ReleaseSession,
}

/// Why a lifecycle request was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunErrorCode {
    /// The request is not valid in the current state.
    InvalidTransition,
    /// A run outcome was reported while no run was active.
    NoActiveRun,
}

impl RunErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTransition => "application.run.invalid_transition",
            Self::NoActiveRun => "application.run.no_active_run",
        }
    }
}

/// Refusal carrying the state and request that produced it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunError {
    code: RunErrorCode,
    state: RunState,
    request: Option<RunRequest>,
}

impl RunError {
    #[must_use]
    pub const fn code(self) -> RunErrorCode {
        self.code
    }

    #[must_use]
    pub const fn state(self) -> RunState {
        self.state
    }

    #[must_use]
    pub const fn request(self) -> Option<RunRequest> {
        self.request
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for RunError {}

/// Terminal outcome reported back by the caller once the backend settles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunOutcome {
    Completed,
    Cancelled,
    Failed,
}

/// Deterministic state machine for one run of one compiled artifact.
///
/// Every accepted request returns the single side effect the caller owes the
/// backend; every refused request leaves the state untouched, so a rejected
/// UI action can never desynchronise the observable lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunOrchestrator {
    state: RunState,
    tick: SimulationTick,
    /// Number of runs started since construction, including restarts.
    started_runs: u64,
}

impl Default for RunOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl RunOrchestrator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: RunState::Idle,
            tick: SimulationTick::new(0),
            started_runs: 0,
        }
    }

    #[must_use]
    pub const fn state(&self) -> RunState {
        self.state
    }

    /// Last tick observed from the backend, reset at the start of every run.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    #[must_use]
    pub const fn started_runs(&self) -> u64 {
        self.started_runs
    }

    /// Returns whether the request would be accepted in the current state.
    #[must_use]
    pub const fn accepts(&self, request: RunRequest) -> bool {
        self.intent_for(request).is_some()
    }

    const fn intent_for(&self, request: RunRequest) -> Option<RunIntent> {
        match (self.state, request) {
            (RunState::Idle, RunRequest::Prepare) => Some(RunIntent::Compile),
            (RunState::Prepared, RunRequest::Start) => Some(RunIntent::StartSession),
            (RunState::Running, RunRequest::Pause) => Some(RunIntent::PauseSession),
            (RunState::Paused, RunRequest::Resume) => Some(RunIntent::ResumeSession),
            (RunState::Running | RunState::Paused, RunRequest::Cancel) => {
                Some(RunIntent::CancelSession)
            }
            (RunState::Completed | RunState::Cancelled | RunState::Failed, RunRequest::Reset) => {
                Some(RunIntent::ReleaseSession)
            }
            _ => None,
        }
    }

    /// Applies one lifecycle request, returning the side effect it owes.
    ///
    /// A refused request is a diagnostic, not a panic: the caller reports the
    /// stable code and the state stays exactly as it was.
    pub fn request(&mut self, request: RunRequest) -> Result<RunIntent, RunError> {
        let Some(intent) = self.intent_for(request) else {
            return Err(RunError {
                code: RunErrorCode::InvalidTransition,
                state: self.state,
                request: Some(request),
            });
        };
        self.state = match request {
            RunRequest::Prepare => RunState::Prepared,
            RunRequest::Start => {
                self.tick = SimulationTick::new(0);
                self.started_runs = self.started_runs.saturating_add(1);
                RunState::Running
            }
            RunRequest::Pause => RunState::Paused,
            RunRequest::Resume => RunState::Running,
            // Cancellation is requested, not yet observed: the run stays
            // active until the backend reports its terminal outcome, so a
            // slow cancel cannot be mistaken for a finished run.
            RunRequest::Cancel => self.state,
            RunRequest::Reset => RunState::Prepared,
        };
        Ok(intent)
    }

    /// Records the tick of an observed frame while a run is active.
    pub fn observe_tick(&mut self, tick: SimulationTick) -> Result<(), RunError> {
        if !matches!(self.state, RunState::Running | RunState::Paused) {
            return Err(RunError {
                code: RunErrorCode::NoActiveRun,
                state: self.state,
                request: None,
            });
        }
        self.tick = tick;
        Ok(())
    }

    /// Records a backend state change that the orchestrator did not request.
    ///
    /// A backend may pause, finish or fail on its own; the observable state
    /// must follow the engine rather than the last user action.
    pub fn observe_session_state(&mut self, state: SessionState) -> Result<(), RunError> {
        match state {
            SessionState::Running | SessionState::Paused
                if !matches!(self.state, RunState::Running | RunState::Paused) =>
            {
                Err(RunError {
                    code: RunErrorCode::NoActiveRun,
                    state: self.state,
                    request: None,
                })
            }
            SessionState::Running => {
                self.state = RunState::Running;
                Ok(())
            }
            SessionState::Paused => {
                self.state = RunState::Paused;
                Ok(())
            }
            SessionState::Completed => self.finish(RunOutcome::Completed),
            SessionState::Cancelled => self.finish(RunOutcome::Cancelled),
            SessionState::Failed => self.finish(RunOutcome::Failed),
        }
    }

    /// Records the single terminal outcome of the active run.
    pub fn finish(&mut self, outcome: RunOutcome) -> Result<(), RunError> {
        if !matches!(self.state, RunState::Running | RunState::Paused) {
            return Err(RunError {
                code: RunErrorCode::NoActiveRun,
                state: self.state,
                request: None,
            });
        }
        self.state = match outcome {
            RunOutcome::Completed => RunState::Completed,
            RunOutcome::Cancelled => RunState::Cancelled,
            RunOutcome::Failed => RunState::Failed,
        };
        Ok(())
    }
}
