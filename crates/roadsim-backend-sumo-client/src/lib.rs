//! SUMO worker-backed implementation of the RoadSim backend contract.
//!
//! This crate is the only place where the backend API, the typed SUMO export
//! and the worker IPC client meet: it materializes export bundles into
//! bounded run directories, runs `netconvert`, drives the out-of-process
//! worker and converts its visual frames back into backend frames. libsumo
//! types never cross this boundary — the worker owns them.

use async_trait::async_trait;
use roadsim_backend_api::{
    Ack, AgentFootprint, AgentState, BACKEND_API_VERSION, BackendArtifact, BackendError,
    BackendErrorCode, BackendErrorPhase, BackendEvent, BackendHello, BackendId, ClientHello,
    CompileOptions, ControlCommand, ControlKind, FrameBatch, RunConfig, RunSummary,
    ScenarioSnapshot, SessionState, SimulationBackend, SimulationSession,
};
use roadsim_backend_sumo::{
    SUMO_CONNECTIONS_FILE, SUMO_EDGES_FILE, SUMO_NETCONVERT_INPUT_ARGUMENTS, SUMO_NODES_FILE,
    SUMO_ROUTES_FILE, SUMO_TLS_FILE, SumoRoadExportOptions, SumoVehicleTypeOptions, export_network,
    export_routes,
};
use roadsim_compiled_network::CompiledNetwork;
use roadsim_types::SimulationTick;
use roadsim_worker_client::{
    RunDirectoryLimits, RunDirectoryManager, WorkerClient, WorkerClientConfig,
};
use roadsim_worker_protocol::{AuthToken, EngineIdentity, WorkerSessionConfig};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const SUMO_BACKEND_ID: BackendId = BackendId::new("roadsim.sumo.v1");
const LIFECYCLE_CAPABILITY: &str = "simulation.lifecycle.step.v1";
const VEHICLE_VISUAL_CAPABILITY: &str = "simulation.visual.vehicle.v1";
const SUMO_CONFIG_FILE: &str = "roadsim.sumocfg";
const MAX_STEPS_PER_EVENT: u64 = 16;
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(2);
/// Bound on how long one event waits for a fresh visual frame; visual frames
/// are latest-wins, so an event without one is legal and non-fatal.
const FRAME_WAIT: Duration = Duration::from_millis(200);

/// How the materialized plain network becomes a runnable `roadsim.net.xml`.
#[derive(Clone, Debug)]
pub enum NetworkMaterialization {
    /// Run the pinned `netconvert` with [`SUMO_NETCONVERT_INPUT_ARGUMENTS`].
    Netconvert(PathBuf),
    /// Write only the plain XML documents and skip `netconvert`.
    ///
    /// Only for workers that do not read the network — protocol test doubles.
    /// A real SUMO worker would fail to open the session, not run silently.
    PlainFilesOnly,
}

/// Explicit configuration of the out-of-process SUMO runner.
#[derive(Clone, Debug)]
pub struct SumoRunnerConfig {
    worker_program: PathBuf,
    worker_arguments: Vec<OsString>,
    materialization: NetworkMaterialization,
    run_root: PathBuf,
    auth_token: AuthToken,
    /// Exact engine the worker must report; `None` accepts any (test doubles).
    engine: Option<EngineIdentity>,
    export: SumoRoadExportOptions,
    vehicle_type: SumoVehicleTypeOptions,
    request_timeout: Duration,
    step_length_ms: u32,
    required_capabilities: Vec<String>,
}

impl SumoRunnerConfig {
    pub fn new(
        worker_program: impl Into<PathBuf>,
        materialization: NetworkMaterialization,
        run_root: impl Into<PathBuf>,
        auth_token: AuthToken,
        export: SumoRoadExportOptions,
        vehicle_type: SumoVehicleTypeOptions,
    ) -> Self {
        Self {
            worker_program: worker_program.into(),
            worker_arguments: Vec::new(),
            materialization,
            run_root: run_root.into(),
            auth_token,
            engine: None,
            export,
            vehicle_type,
            request_timeout: Duration::from_secs(10),
            step_length_ms: 100,
            required_capabilities: vec![
                LIFECYCLE_CAPABILITY.to_owned(),
                VEHICLE_VISUAL_CAPABILITY.to_owned(),
            ],
        }
    }

    /// Overrides the capabilities requested from the worker; a protocol test
    /// double announces its own capability names.
    #[must_use]
    pub fn with_required_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.required_capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn with_worker_argument(mut self, argument: impl Into<OsString>) -> Self {
        self.worker_arguments.push(argument.into());
        self
    }

    #[must_use]
    pub fn with_engine(mut self, engine: EngineIdentity) -> Self {
        self.engine = Some(engine);
        self
    }

    #[must_use]
    pub const fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

struct PreparedRun {
    directory: PathBuf,
    /// Keeps the journal guard alive so the run is not recovered as
    /// interrupted while it is merely prepared or executing.
    guard: Mutex<roadsim_worker_client::RunDirectory>,
}

/// SUMO backend: compiles CSNs into run directories and runs the worker.
pub struct SumoRunnerBackend {
    config: SumoRunnerConfig,
    prepared: Mutex<BTreeMap<(String, String), Arc<PreparedRun>>>,
    next_run_id: AtomicU64,
}

impl SumoRunnerBackend {
    #[must_use]
    pub fn new(config: SumoRunnerConfig) -> Self {
        Self {
            config,
            prepared: Mutex::new(BTreeMap::new()),
            next_run_id: AtomicU64::new(1),
        }
    }

    fn compile_error(code: BackendErrorCode) -> BackendError {
        BackendError::new(BackendErrorPhase::Compile, code)
    }
}

#[async_trait]
impl SimulationBackend for SumoRunnerBackend {
    async fn handshake(&self, client: ClientHello) -> Result<BackendHello, BackendError> {
        if client.api_version() != BACKEND_API_VERSION {
            return Err(BackendError::new(
                BackendErrorPhase::Handshake,
                BackendErrorCode::ProtocolMismatch,
            ));
        }
        Ok(BackendHello::new(
            SUMO_BACKEND_ID,
            [roadsim_compiled_network::CapabilityId::RoadVehiclesBasic],
            true,
        ))
    }

    async fn compile(
        &self,
        network: Arc<CompiledNetwork>,
        scenario: ScenarioSnapshot,
        options: CompileOptions,
    ) -> Result<BackendArtifact, BackendError> {
        let bundle = export_network(&network, self.config.export)
            .map_err(|_| Self::compile_error(BackendErrorCode::EmptyNetwork))?;
        let routes = options
            .demand()
            .map(|demand| export_routes(&network, demand, &bundle, self.config.vehicle_type))
            .transpose()
            .map_err(|_| Self::compile_error(BackendErrorCode::InvalidScenario))?;

        let run_id = self.next_run_id.fetch_add(1, Ordering::Relaxed);
        let manager = RunDirectoryManager::new(
            &self.config.run_root,
            RunDirectoryLimits::new(4, 16)
                .map_err(|_| Self::compile_error(BackendErrorCode::InternalState))?,
        )
        .map_err(|_| Self::compile_error(BackendErrorCode::InternalState))?;
        let run = manager
            .create_run(run_id, run_id)
            .map_err(|_| Self::compile_error(BackendErrorCode::InternalState))?;
        let directory = run.path().to_path_buf();

        let write = |name: &str, content: &str| {
            std::fs::write(directory.join(name), content)
                .map_err(|_| Self::compile_error(BackendErrorCode::InternalState))
        };
        write(SUMO_NODES_FILE, bundle.nodes_xml())?;
        write(SUMO_EDGES_FILE, bundle.edges_xml())?;
        write(SUMO_CONNECTIONS_FILE, bundle.connections_xml())?;
        write(SUMO_TLS_FILE, bundle.tls_xml())?;
        let route_input = if let Some(routes) = &routes {
            write(SUMO_ROUTES_FILE, routes.routes_xml())?;
            format!("<route-files value=\"{SUMO_ROUTES_FILE}\"/>")
        } else {
            String::new()
        };
        write(
            SUMO_CONFIG_FILE,
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<configuration>\n    <input><net-file value=\"roadsim.net.xml\"/>{route_input}</input>\n</configuration>\n"
            ),
        )?;

        if let NetworkMaterialization::Netconvert(netconvert) = &self.config.materialization {
            let output = Command::new(netconvert)
                .current_dir(&directory)
                .args(SUMO_NETCONVERT_INPUT_ARGUMENTS)
                .args(["--output-file", "roadsim.net.xml"])
                .output()
                .map_err(|_| Self::compile_error(BackendErrorCode::InternalState))?;
            if !output.status.success() {
                return Err(Self::compile_error(BackendErrorCode::InvalidScenario));
            }
        }

        let artifact = BackendArtifact::new(
            SUMO_BACKEND_ID,
            network.header().content_hash(),
            scenario.content_hash(),
        );
        self.prepared.lock().expect("prepared map lock").insert(
            (
                network.header().content_hash().to_string(),
                scenario.content_hash().to_string(),
            ),
            Arc::new(PreparedRun {
                directory,
                guard: Mutex::new(run),
            }),
        );
        Ok(artifact)
    }

    async fn start(
        &self,
        artifact: BackendArtifact,
        run: RunConfig,
    ) -> Result<Box<dyn SimulationSession>, BackendError> {
        if artifact.backend_id() != SUMO_BACKEND_ID {
            return Err(BackendError::new(
                BackendErrorPhase::Runtime,
                BackendErrorCode::ArtifactBackendMismatch,
            ));
        }
        let prepared = self
            .prepared
            .lock()
            .expect("prepared map lock")
            .get(&(
                artifact.network_hash().to_string(),
                artifact.scenario_hash().to_string(),
            ))
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    BackendErrorPhase::Runtime,
                    BackendErrorCode::ArtifactNotFound,
                )
            })?;

        let mut worker_config =
            WorkerClientConfig::new(&self.config.worker_program, self.config.auth_token.clone())
                .with_work_directory(&prepared.directory);
        for argument in &self.config.worker_arguments {
            worker_config = worker_config.with_argument(argument.clone());
        }
        let runtime_error =
            |code: BackendErrorCode| BackendError::new(BackendErrorPhase::Runtime, code);
        let mut client = WorkerClient::spawn(worker_config)
            .map_err(|_| runtime_error(BackendErrorCode::InternalState))?;
        let capabilities = self.config.required_capabilities.clone();
        let handshake = match &self.config.engine {
            Some(engine) => client.handshake_with_engine(
                capabilities,
                engine.clone(),
                self.config.request_timeout,
            ),
            None => client.handshake(capabilities, self.config.request_timeout),
        };
        if handshake.is_err() {
            return Err(runtime_error(BackendErrorCode::UnsupportedCapabilities));
        }
        let session_config = WorkerSessionConfig::new(
            SUMO_CONFIG_FILE,
            run.root_seed().get(),
            self.config.step_length_ms,
        )
        .map_err(|_| runtime_error(BackendErrorCode::InvalidRunConfig))?;
        let session_id = self.next_run_id.fetch_add(1, Ordering::Relaxed);
        client
            .open_session(session_id, session_config, self.config.request_timeout)
            .map_err(|_| runtime_error(BackendErrorCode::InvalidLifecycle))?;
        let _ = prepared
            .guard
            .lock()
            .expect("run guard lock")
            .mark_running();

        Ok(Box::new(SumoSession {
            run: prepared.clone(),
            client,
            session_id,
            state: SessionState::Running,
            tick: SimulationTick::new(0),
            remaining_ticks: run.duration_ticks(),
            step_ticks: run.step_ticks(),
            emitted_frames: 0,
            timeout: self.config.request_timeout,
            announced_running: false,
        }))
    }
}

/// One live worker-backed run. Stepping is client-driven, so pausing simply
/// stops issuing steps; the worker holds a consistent engine state meanwhile.
struct SumoSession {
    run: Arc<PreparedRun>,
    client: WorkerClient,
    session_id: u64,
    state: SessionState,
    tick: SimulationTick,
    remaining_ticks: u64,
    step_ticks: u64,
    emitted_frames: u64,
    timeout: Duration,
    announced_running: bool,
}

impl SumoSession {
    fn summary(&self) -> RunSummary {
        RunSummary::new(self.state, self.tick, self.emitted_frames)
    }

    fn fail(&mut self) -> BackendError {
        self.state = SessionState::Failed;
        let _ = self
            .run
            .guard
            .lock()
            .expect("run guard lock")
            .mark_failed("backend.sumo_client.worker_failure");
        BackendError::new(BackendErrorPhase::Runtime, BackendErrorCode::InternalState)
    }

    /// Newest visual frame, or `None` when the bounded wait elapses.
    ///
    /// An invalid agent in a frame is a backend error, not a dropped frame.
    fn latest_frame(&mut self, wait: Duration) -> Result<Option<FrameBatch>, BackendError> {
        let deadline = Instant::now() + wait;
        loop {
            if let Some((session_id, _, batch)) = self.client.take_latest_visual_frame() {
                if session_id != self.session_id {
                    continue;
                }
                let mut agents = Vec::with_capacity(batch.agent_ids().len());
                for index in 0..batch.agent_ids().len() {
                    let footprint =
                        AgentFootprint::new(batch.length_m()[index], batch.width_m()[index])
                            .map_err(|_| self.fail())?;
                    agents.push(
                        AgentState::new(
                            batch.agent_ids()[index],
                            None,
                            batch.x_m()[index],
                            batch.y_m()[index],
                            batch.heading_rad()[index],
                            footprint,
                        )
                        .map_err(|_| self.fail())?,
                    );
                }
                return Ok(Some(FrameBatch::new(
                    SimulationTick::new(batch.tick()),
                    agents,
                )));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(FRAME_POLL_INTERVAL);
        }
    }

    /// Ends the worker lifecycle; cancellation already closed the session on
    /// the worker side, so only completion sends an explicit close.
    fn finish(&mut self, state: SessionState) -> Result<(), BackendError> {
        if state == SessionState::Completed
            && self
                .client
                .close_session(self.session_id, self.timeout)
                .is_err()
        {
            return Err(self.fail());
        }
        if self.client.shutdown(self.timeout).is_err() {
            return Err(self.fail());
        }
        self.state = state;
        let _ = self
            .run
            .guard
            .lock()
            .expect("run guard lock")
            .mark_completed();
        Ok(())
    }
}

#[async_trait]
impl SimulationSession for SumoSession {
    fn state(&self) -> SessionState {
        self.state
    }

    async fn control(&mut self, command: ControlCommand) -> Result<Ack, BackendError> {
        if self.state.is_terminal() {
            return Err(BackendError::new(
                BackendErrorPhase::Runtime,
                BackendErrorCode::TerminalSession,
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
        Ok(Ack::new(command.sequence(), false))
    }

    async fn next_event(&mut self) -> Result<BackendEvent, BackendError> {
        match self.state {
            SessionState::Paused => return Ok(BackendEvent::StateChanged(SessionState::Paused)),
            state if state.is_terminal() => {
                return Ok(BackendEvent::Completed(self.summary()));
            }
            _ => {}
        }
        if !self.announced_running {
            self.announced_running = true;
            return Ok(BackendEvent::StateChanged(SessionState::Running));
        }
        if self.remaining_ticks == 0 {
            self.finish(SessionState::Completed)?;
            return Ok(BackendEvent::Completed(self.summary()));
        }
        let steps = self
            .step_ticks
            .max(1)
            .min(self.remaining_ticks)
            .min(MAX_STEPS_PER_EVENT);
        let steps_u32 = u32::try_from(steps).map_err(|_| self.fail())?;
        let tick = self
            .client
            .step_session(self.session_id, steps_u32, self.timeout)
            .map_err(|_| self.fail())?;
        self.tick = SimulationTick::new(tick);
        self.remaining_ticks -= steps;
        // Visual frames are latest-wins; a missing frame after the bounded
        // wait is reported as an empty batch at the stepped tick, never as an
        // invented agent set.
        let frame = self
            .latest_frame(FRAME_WAIT)?
            .unwrap_or_else(|| FrameBatch::new(self.tick, Vec::new()));
        self.emitted_frames += 1;
        Ok(BackendEvent::Frame(frame))
    }

    async fn cancel(&mut self) -> Result<RunSummary, BackendError> {
        if !self.state.is_terminal() {
            let cancel = self.client.cancel_session(self.session_id, self.timeout);
            if cancel.is_err() {
                return Err(self.fail());
            }
            self.finish(SessionState::Cancelled)?;
        }
        Ok(self.summary())
    }
}
