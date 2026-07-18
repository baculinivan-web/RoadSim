//! Atomic, backend-independent commands are the only mutation path used by UI.

use roadsim_domain::{Corridor, DesignCatalog, Project};
use roadsim_types::{CorridorId, ObjectRef, ProjectId};
use std::{
    collections::{BTreeSet, VecDeque},
    error::Error,
    fmt,
    num::NonZeroUsize,
    sync::Arc,
};

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
    UndoUnavailable,
    RedoUnavailable,
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
            Self::UndoUnavailable => "command.history.undo_unavailable",
            Self::RedoUnavailable => "command.history.redo_unavailable",
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandOutcome {
    changed: BTreeSet<ObjectRef>,
    created: Vec<ObjectRef>,
    deleted: Vec<ObjectRef>,
    inverse: InverseCommand,
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
        other
            .inverse
            .operations
            .append(&mut self.inverse.operations);
        self.inverse = other.inverse;
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
    #[must_use]
    pub const fn inverse(&self) -> &InverseCommand {
        &self.inverse
    }
    #[must_use]
    pub fn into_inverse(self) -> InverseCommand {
        self.inverse
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PrimitiveCommand {
    Create(Corridor),
    Update(Corridor),
    Delete(CorridorId),
}

/// A command sequence that restores the semantic state preceding an operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InverseCommand {
    operations: Vec<PrimitiveCommand>,
}

impl InverseCommand {
    fn single(operation: PrimitiveCommand) -> Self {
        Self {
            operations: vec![operation],
        }
    }

    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.operations.len()
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

mod sealed {
    pub trait Sealed {}
}

/// A typed domain command. UI-specific gestures never enter this sealed contract.
pub trait Command: sealed::Sealed {
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

impl sealed::Sealed for CreateCorridor {}

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
            inverse: InverseCommand::single(PrimitiveCommand::Delete(id)),
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

impl sealed::Sealed for UpdateCorridor {}

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
        let previous = std::mem::replace(&mut model.corridors[index], self.corridor.clone());
        let object_ref = ObjectRef::from(id);
        Ok(CommandOutcome {
            changed: BTreeSet::from([object_ref]),
            created: Vec::new(),
            deleted: Vec::new(),
            inverse: InverseCommand::single(PrimitiveCommand::Update(previous)),
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

impl sealed::Sealed for DeleteCorridor {}

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
        let deleted = model.corridors.remove(index);
        let object_ref = ObjectRef::from(self.id);
        Ok(CommandOutcome {
            changed: BTreeSet::from([object_ref]),
            created: Vec::new(),
            deleted: vec![object_ref],
            inverse: InverseCommand::single(PrimitiveCommand::Create(deleted)),
        })
    }
}

impl sealed::Sealed for InverseCommand {}

impl Command for InverseCommand {
    fn apply(&self, model: &mut TransactionModel) -> Result<CommandOutcome, CommandError> {
        if self.operations.is_empty() {
            return Err(CommandError::new(
                CommandErrorCode::EmptyTransaction,
                Vec::new(),
            ));
        }
        let mut combined = CommandOutcome::default();
        for operation in &self.operations {
            let outcome = match operation {
                PrimitiveCommand::Create(corridor) => {
                    CreateCorridor::new(corridor.clone()).apply(model)
                }
                PrimitiveCommand::Update(corridor) => {
                    UpdateCorridor::new(corridor.clone()).apply(model)
                }
                PrimitiveCommand::Delete(id) => DeleteCorridor::new(*id).apply(model),
            }?;
            combined.merge(outcome);
        }
        Ok(combined)
    }
}

fn rebuild_project(project: &Project, catalog: DesignCatalog) -> Result<Project, CommandError> {
    Project::with_catalog(
        project.id(),
        project.metadata().clone(),
        project.coordinate_reference().clone(),
        catalog,
    )
    .with_study_catalog(project.study_catalog().clone())
    .map_err(|error| {
        CommandError::new(
            CommandErrorCode::DomainInvariant,
            error.object_refs().to_vec(),
        )
    })
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
        let catalog = self
            .project_shell
            .design_catalog()
            .replace_corridors(self.working_model.corridors)
            .map_err(|error| {
                CommandError::new(
                    CommandErrorCode::DomainInvariant,
                    error.object_refs().to_vec(),
                )
            })?;
        state.project = rebuild_project(&self.project_shell, catalog)?;
        state.revision = next_revision;
        Ok(self.outcome)
    }
}

/// Bounded, state-lineage-specific undo/redo history.
#[derive(Debug)]
pub struct CommandHistory {
    state_identity: StateIdentity,
    project_id: ProjectId,
    observed_revision: ModelRevision,
    capacity: NonZeroUsize,
    undo: VecDeque<InverseCommand>,
    redo: VecDeque<InverseCommand>,
}

impl CommandHistory {
    #[must_use]
    pub fn new(state: &ModelState, capacity: NonZeroUsize) -> Self {
        Self {
            state_identity: state.identity.clone(),
            project_id: state.project.id(),
            observed_revision: state.revision,
            capacity,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn execute<C: Command>(
        &mut self,
        state: &mut ModelState,
        command: &C,
    ) -> Result<CommandOutcome, CommandError> {
        self.ensure_state(state)?;
        let outcome = state.execute(command)?;
        self.observed_revision = state.revision;
        self.record_new(outcome.inverse().clone());
        Ok(outcome)
    }

    pub fn execute_envelope<C: Command>(
        &mut self,
        state: &mut ModelState,
        envelope: &CommandEnvelope<C>,
    ) -> Result<CommandOutcome, CommandError> {
        self.ensure_state(state)?;
        let outcome = state.execute_envelope(envelope)?;
        self.observed_revision = state.revision;
        self.record_new(outcome.inverse().clone());
        Ok(outcome)
    }

    pub fn commit_transaction(
        &mut self,
        state: &mut ModelState,
        transaction: ModelTransaction,
    ) -> Result<CommandOutcome, CommandError> {
        self.ensure_state(state)?;
        let outcome = transaction.commit(state)?;
        self.observed_revision = state.revision;
        self.record_new(outcome.inverse().clone());
        Ok(outcome)
    }

    pub fn undo(&mut self, state: &mut ModelState) -> Result<CommandOutcome, CommandError> {
        self.ensure_state(state)?;
        let command = self
            .undo
            .back()
            .cloned()
            .ok_or_else(|| CommandError::new(CommandErrorCode::UndoUnavailable, Vec::new()))?;
        let outcome = state.execute(&command)?;
        self.observed_revision = state.revision;
        self.undo.pop_back();
        Self::push_bounded(&mut self.redo, self.capacity, outcome.inverse().clone());
        Ok(outcome)
    }

    pub fn redo(&mut self, state: &mut ModelState) -> Result<CommandOutcome, CommandError> {
        self.ensure_state(state)?;
        let command = self
            .redo
            .back()
            .cloned()
            .ok_or_else(|| CommandError::new(CommandErrorCode::RedoUnavailable, Vec::new()))?;
        let outcome = state.execute(&command)?;
        self.observed_revision = state.revision;
        self.redo.pop_back();
        Self::push_bounded(&mut self.undo, self.capacity, outcome.inverse().clone());
        Ok(outcome)
    }

    fn ensure_state(&self, state: &ModelState) -> Result<(), CommandError> {
        if self.state_identity == state.identity {
            if self.observed_revision == state.revision {
                return Ok(());
            }
            return Err(CommandError::new(
                CommandErrorCode::StaleRevision,
                vec![state.project.id().into()],
            ));
        }
        Err(CommandError::new(
            CommandErrorCode::WrongState,
            vec![self.project_id.into(), state.project.id().into()],
        ))
    }

    fn record_new(&mut self, inverse: InverseCommand) {
        Self::push_bounded(&mut self.undo, self.capacity, inverse);
        self.redo.clear();
    }

    fn push_bounded(
        queue: &mut VecDeque<InverseCommand>,
        capacity: NonZeroUsize,
        command: InverseCommand,
    ) {
        if queue.len() == capacity.get() {
            queue.pop_front();
        }
        queue.push_back(command);
    }
}
