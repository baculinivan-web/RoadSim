//! Atomic, backend-independent commands are the only mutation path used by UI.

use roadsim_domain::{Corridor, DesignCatalog, Project};
use roadsim_types::{CorridorId, ObjectRef, ProjectId};
use std::{collections::BTreeSet, error::Error, fmt, sync::Arc};

#[derive(Clone, Debug)]
struct StateIdentity(Arc<()>);

impl StateIdentity {
    fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for StateIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for StateIdentity {}

/// Monotonic in-memory Design Model revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModelRevision(u64);

impl ModelRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Stable command and transaction failure classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandErrorCode {
    CorridorAlreadyExists,
    CorridorNotFound,
    DomainInvariant,
    TransactionAborted,
    EmptyTransaction,
    WrongState,
    StaleRevision,
    RevisionOverflow,
}

impl CommandErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CorridorAlreadyExists => "command.corridor.already_exists",
            Self::CorridorNotFound => "command.corridor.not_found",
            Self::DomainInvariant => "command.domain_invariant",
            Self::TransactionAborted => "command.transaction.aborted",
            Self::EmptyTransaction => "command.transaction.empty",
            Self::WrongState => "command.transaction.wrong_state",
            Self::StaleRevision => "command.transaction.stale_revision",
            Self::RevisionOverflow => "command.revision.overflow",
        }
    }
}

/// Machine-readable command diagnostic with stable affected object references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandError {
    code: CommandErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl CommandError {
    fn new(code: CommandErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }
    #[must_use]
    pub const fn code(&self) -> CommandErrorCode {
        self.code
    }
    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for CommandError {}

/// Observable effects of one committed logical transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandOutcome {
    changed: BTreeSet<ObjectRef>,
    created: Vec<ObjectRef>,
    deleted: Vec<ObjectRef>,
}

impl CommandOutcome {
    fn merge(&mut self, mut other: Self) {
        self.changed.append(&mut other.changed);
        self.created.append(&mut other.created);
        self.deleted.append(&mut other.deleted);
        self.created.sort_unstable();
        self.created.dedup();
        self.deleted.sort_unstable();
        self.deleted.dedup();
    }
    #[must_use]
    pub const fn changed(&self) -> &BTreeSet<ObjectRef> {
        &self.changed
    }
    #[must_use]
    pub fn created(&self) -> &[ObjectRef] {
        &self.created
    }
    #[must_use]
    pub fn deleted(&self) -> &[ObjectRef] {
        &self.deleted
    }
}

/// Mutable transaction-local Design Model projection.
///
/// It may be temporarily structurally inconsistent while an atomic batch is
/// assembled. A transaction validates the complete catalog before publishing.
#[derive(Clone, Debug)]
pub struct TransactionModel {
    corridors: Vec<Corridor>,
}

impl TransactionModel {
    #[must_use]
    pub fn corridors(&self) -> &[Corridor] {
        &self.corridors
    }
}

/// A typed domain command. UI-specific gestures never enter this contract.
pub trait Command {
    fn apply(&self, model: &mut TransactionModel) -> Result<CommandOutcome, CommandError>;
}

/// Application envelope binding a command to the revision observed by its sender.
#[derive(Clone, Debug)]
pub struct CommandEnvelope<C> {
    target_identity: StateIdentity,
    target_project_id: ProjectId,
    expected_revision: ModelRevision,
    command: C,
}

impl<C> CommandEnvelope<C> {
    #[must_use]
    pub const fn expected_revision(&self) -> ModelRevision {
        self.expected_revision
    }
    #[must_use]
    pub const fn target_project_id(&self) -> ProjectId {
        self.target_project_id
    }
    #[must_use]
    pub const fn command(&self) -> &C {
        &self.command
    }
}

#[derive(Clone, Debug)]
pub struct CreateCorridor {
    corridor: Corridor,
}
impl CreateCorridor {
    #[must_use]
    pub const fn new(corridor: Corridor) -> Self {
        Self { corridor }
    }
}

impl Command for CreateCorridor {
    fn apply(&self, model: &mut TransactionModel) -> Result<CommandOutcome, CommandError> {
        let id = self.corridor.id();
        if model.corridors.iter().any(|corridor| corridor.id() == id) {
            return Err(CommandError::new(
                CommandErrorCode::CorridorAlreadyExists,
                vec![id.into()],
            ));
        }
        model.corridors.push(self.corridor.clone());
        let object_ref = ObjectRef::from(id);
        Ok(CommandOutcome {
            changed: BTreeSet::from([object_ref]),
            created: vec![object_ref],
            deleted: Vec::new(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct UpdateCorridor {
    corridor: Corridor,
}
impl UpdateCorridor {
    #[must_use]
    pub const fn new(corridor: Corridor) -> Self {
        Self { corridor }
    }
}

impl Command for UpdateCorridor {
    fn apply(&self, model: &mut TransactionModel) -> Result<CommandOutcome, CommandError> {
        let id = self.corridor.id();
        let Some(index) = model
            .corridors
            .iter()
            .position(|corridor| corridor.id() == id)
        else {
            return Err(CommandError::new(
                CommandErrorCode::CorridorNotFound,
                vec![id.into()],
            ));
        };
        model.corridors[index] = self.corridor.clone();
        let object_ref = ObjectRef::from(id);
        Ok(CommandOutcome {
            changed: BTreeSet::from([object_ref]),
            created: Vec::new(),
            deleted: Vec::new(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeleteCorridor {
    id: CorridorId,
}
impl DeleteCorridor {
    #[must_use]
    pub const fn new(id: CorridorId) -> Self {
        Self { id }
    }
}

impl Command for DeleteCorridor {
    fn apply(&self, model: &mut TransactionModel) -> Result<CommandOutcome, CommandError> {
        let Some(index) = model
            .corridors
            .iter()
            .position(|corridor| corridor.id() == self.id)
        else {
            return Err(CommandError::new(
                CommandErrorCode::CorridorNotFound,
                vec![self.id.into()],
            ));
        };
        model.corridors.remove(index);
        let object_ref = ObjectRef::from(self.id);
        Ok(CommandOutcome {
            changed: BTreeSet::from([object_ref]),
            created: Vec::new(),
            deleted: vec![object_ref],
        })
    }
}

fn rebuild_project(project: &Project, catalog: DesignCatalog) -> Project {
    Project::with_catalog(
        project.id(),
        project.metadata().clone(),
        project.coordinate_reference().clone(),
        catalog,
    )
}

/// Owned project state exposed to application orchestration.
#[derive(Debug)]
pub struct ModelState {
    project: Project,
    revision: ModelRevision,
    identity: StateIdentity,
}

impl Clone for ModelState {
    fn clone(&self) -> Self {
        Self {
            project: self.project.clone(),
            revision: self.revision,
            identity: StateIdentity::new(),
        }
    }
}

impl PartialEq for ModelState {
    fn eq(&self, other: &Self) -> bool {
        self.project == other.project && self.revision == other.revision
    }
}

impl ModelState {
    #[must_use]
    pub fn new(project: Project) -> Self {
        Self {
            project,
            revision: ModelRevision::initial(),
            identity: StateIdentity::new(),
        }
    }
    #[must_use]
    pub fn with_revision(project: Project, revision: ModelRevision) -> Self {
        Self {
            project,
            revision,
            identity: StateIdentity::new(),
        }
    }
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }
    #[must_use]
    pub const fn revision(&self) -> ModelRevision {
        self.revision
    }
    /// Binds a command to this exact model lineage and observed revision.
    #[must_use]
    pub fn envelope<C>(&self, command: C) -> CommandEnvelope<C> {
        CommandEnvelope {
            target_identity: self.identity.clone(),
            target_project_id: self.project.id(),
            expected_revision: self.revision,
            command,
        }
    }
    #[must_use]
    pub fn begin_transaction(&self) -> ModelTransaction {
        ModelTransaction {
            source_identity: self.identity.clone(),
            base_revision: self.revision,
            project_shell: self.project.clone(),
            working_model: TransactionModel {
                corridors: self.project.design_catalog().corridors().to_vec(),
            },
            outcome: CommandOutcome::default(),
            command_count: 0,
            aborted: false,
        }
    }
    pub fn execute<C: Command>(&mut self, command: &C) -> Result<CommandOutcome, CommandError> {
        let mut transaction = self.begin_transaction();
        transaction.apply(command)?;
        transaction.commit(self)
    }

    pub fn execute_envelope<C: Command>(
        &mut self,
        envelope: &CommandEnvelope<C>,
    ) -> Result<CommandOutcome, CommandError> {
        if self.identity != envelope.target_identity {
            return Err(CommandError::new(
                CommandErrorCode::WrongState,
                vec![envelope.target_project_id.into(), self.project.id().into()],
            ));
        }
        if envelope.expected_revision() != self.revision {
            return Err(CommandError::new(
                CommandErrorCode::StaleRevision,
                Vec::new(),
            ));
        }
        self.execute(envelope.command())
    }
}

/// Isolated working copy published only by a successful commit.
pub struct ModelTransaction {
    source_identity: StateIdentity,
    base_revision: ModelRevision,
    project_shell: Project,
    working_model: TransactionModel,
    outcome: CommandOutcome,
    command_count: usize,
    aborted: bool,
}

impl ModelTransaction {
    pub fn apply<C: Command>(&mut self, command: &C) -> Result<(), CommandError> {
        if self.aborted {
            return Err(CommandError::new(
                CommandErrorCode::TransactionAborted,
                Vec::new(),
            ));
        }
        match command.apply(&mut self.working_model) {
            Ok(outcome) => {
                self.outcome.merge(outcome);
                self.command_count += 1;
                Ok(())
            }
            Err(error) => {
                self.aborted = true;
                Err(error)
            }
        }
    }

    pub fn commit(self, state: &mut ModelState) -> Result<CommandOutcome, CommandError> {
        if self.aborted {
            return Err(CommandError::new(
                CommandErrorCode::TransactionAborted,
                Vec::new(),
            ));
        }
        if self.command_count == 0 {
            return Err(CommandError::new(
                CommandErrorCode::EmptyTransaction,
                Vec::new(),
            ));
        }
        if state.identity != self.source_identity {
            return Err(CommandError::new(
                CommandErrorCode::WrongState,
                vec![self.project_shell.id().into(), state.project.id().into()],
            ));
        }
        if state.revision != self.base_revision {
            return Err(CommandError::new(
                CommandErrorCode::StaleRevision,
                Vec::new(),
            ));
        }
        let next_revision = state
            .revision
            .checked_next()
            .ok_or_else(|| CommandError::new(CommandErrorCode::RevisionOverflow, Vec::new()))?;
        let catalog = DesignCatalog::new(self.working_model.corridors).map_err(|error| {
            let mut refs = Vec::new();
            if let Some(corridor) = error.corridor_id() {
                refs.push(corridor.into());
            }
            if let Some(lane) = error.lane_id() {
                refs.push(lane.into());
            }
            CommandError::new(CommandErrorCode::DomainInvariant, refs)
        })?;
        state.project = rebuild_project(&self.project_shell, catalog);
        state.revision = next_revision;
        Ok(self.outcome)
    }
}
