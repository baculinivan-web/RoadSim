use proptest::prelude::*;
use roadsim_commands::{
    CommandErrorCode, CommandHistory, CreateCorridor, DeleteCorridor, ModelRevision, ModelState,
    UpdateCorridor,
};
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CrossSectionLayout,
    CrossSectionProfile, CrossSectionSection, CrsDefinition, CrsProvenance, DesignCatalog,
    EngineeringCrsDescriptor, EngineeringUnit, LaneDefinition, LaneDirection, LaneSlice, LaneUse,
    LocalOrigin, Point2Meters, Project, ProjectMetadata, ReferenceLine, ReferenceLineElement,
    ReferenceLinePose, VerticalDatum,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, HeadingRadians, LaneId, LengthMeters, ProjectId,
};
use std::num::NonZeroUsize;

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}

fn empty_project() -> Project {
    let source = CrsDefinition::Authority(AuthorityCrs::new("EPSG", "4326").unwrap());
    let engineering = EngineeringCrsDescriptor::new(
        CrsDefinition::Authority(AuthorityCrs::new("EPSG", "32637").unwrap()),
        EngineeringUnit::Metre,
    )
    .unwrap();
    Project::new(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Command test").unwrap(),
        CoordinateReference::new(
            source,
            engineering,
            LocalOrigin::new(
                CoordinateMeters::try_new(0.0).unwrap(),
                CoordinateMeters::try_new(0.0).unwrap(),
            ),
            AxisOrder::EastNorth,
            VerticalDatum::not_specified(),
            CrsProvenance::new("fixture", "identity", "east-north").unwrap(),
        ),
    )
}

fn corridor_with_ids(corridor_id: u128, lane_id: u128, width: f64) -> Corridor {
    let corridor_id = CorridorId::from_u128(corridor_id);
    let lane_id = LaneId::from_u128(lane_id);
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(0.0).unwrap(),
                CoordinateMeters::try_new(0.0).unwrap(),
            ),
            HeadingRadians::try_new(0.0).unwrap(),
        ),
        vec![ReferenceLineElement::line(length(50.0)).unwrap()],
    )
    .unwrap();
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0),
        CrossSectionLayout::new(
            vec![],
            vec![LaneSlice::new(lane_id, length(width)).unwrap()],
        )
        .unwrap(),
    )])
    .unwrap();
    Corridor::new(
        corridor_id,
        reference_line,
        vec![LaneDefinition::new(
            lane_id,
            LaneDirection::AlongReference,
            LaneUse::GeneralTraffic,
        )],
        profile,
    )
    .unwrap()
}

fn corridor(width: f64) -> Corridor {
    corridor_with_ids(10, 20, width)
}

fn project_with_corridors(corridors: Vec<Corridor>) -> Project {
    let project = empty_project();
    Project::with_catalog(
        project.id(),
        project.metadata().clone(),
        project.coordinate_reference().clone(),
        DesignCatalog::new(corridors).unwrap(),
    )
}

#[test]
fn create_update_delete_are_atomic_and_revisioned() {
    let mut state = ModelState::new(empty_project());
    let created = state.execute(&CreateCorridor::new(corridor(3.0))).unwrap();
    assert_eq!(state.revision().get(), 1);
    assert_eq!(created.created().len(), 1);
    assert!(
        state
            .project()
            .design_catalog()
            .corridor(CorridorId::from_u128(10))
            .is_some()
    );

    let updated = state.execute(&UpdateCorridor::new(corridor(3.5))).unwrap();
    assert_eq!(state.revision().get(), 2);
    assert_eq!(updated.changed().len(), 1);
    let stored = state
        .project()
        .design_catalog()
        .corridor(CorridorId::from_u128(10))
        .unwrap();
    assert_eq!(
        stored.cross_section_profile().sections()[0]
            .layout()
            .total_width()
            .get(),
        3.5
    );

    let deleted = state
        .execute(&DeleteCorridor::new(CorridorId::from_u128(10)))
        .unwrap();
    assert_eq!(state.revision().get(), 3);
    assert_eq!(deleted.deleted().len(), 1);
    assert!(state.project().design_catalog().corridors().is_empty());
}

#[test]
fn failed_commands_leave_project_and_revision_unchanged() {
    let mut state = ModelState::new(empty_project());
    state.execute(&CreateCorridor::new(corridor(3.0))).unwrap();
    let before = state.clone();

    let duplicate = state
        .execute(&CreateCorridor::new(corridor(3.0)))
        .unwrap_err();
    assert_eq!(duplicate.code(), CommandErrorCode::CorridorAlreadyExists);
    assert_eq!(duplicate.object_refs().len(), 1);
    assert_eq!(state, before);

    let mut empty = ModelState::new(empty_project());
    let missing = empty
        .execute(&DeleteCorridor::new(CorridorId::from_u128(999)))
        .unwrap_err();
    assert_eq!(missing.code(), CommandErrorCode::CorridorNotFound);
    assert_eq!(empty.revision().get(), 0);
    assert!(empty.project().design_catalog().corridors().is_empty());

    let missing_update = empty
        .execute(&UpdateCorridor::new(corridor(3.0)))
        .unwrap_err();
    assert_eq!(missing_update.code(), CommandErrorCode::CorridorNotFound);
    assert_eq!(empty.revision().get(), 0);
}

#[test]
fn revision_overflow_cannot_publish_working_state() {
    let project = empty_project();
    let mut state = ModelState::with_revision(project.clone(), ModelRevision::new(u64::MAX));
    let error = state
        .execute(&CreateCorridor::new(corridor(3.0)))
        .unwrap_err();
    assert_eq!(error.code(), CommandErrorCode::RevisionOverflow);
    assert_eq!(state.project(), &project);
    assert_eq!(state.revision().get(), u64::MAX);
}

#[test]
fn command_envelope_rejects_a_sender_with_stale_revision() {
    let mut state = ModelState::new(empty_project());
    let envelope = state.envelope(CreateCorridor::new(corridor(3.0)));
    state.execute(&CreateCorridor::new(corridor(4.0))).unwrap();
    let before = state.clone();
    let error = state.execute_envelope(&envelope).unwrap_err();
    assert_eq!(error.code(), CommandErrorCode::StaleRevision);
    assert_eq!(state, before);
}

#[test]
fn command_envelope_rejects_an_unrelated_state_at_the_same_revision() {
    let source = ModelState::new(empty_project());
    let envelope = source.envelope(CreateCorridor::new(corridor(3.0)));
    let mut unrelated = ModelState::new(Project::with_catalog(
        ProjectId::from_u128(2),
        source.project().metadata().clone(),
        source.project().coordinate_reference().clone(),
        DesignCatalog::empty(),
    ));
    let before = unrelated.clone();

    let error = unrelated.execute_envelope(&envelope).unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::WrongState);
    assert_eq!(error.object_refs().len(), 2);
    assert_eq!(unrelated, before);
}

#[test]
fn any_failed_step_aborts_the_whole_transaction() {
    let mut state = ModelState::new(empty_project());
    let before = state.clone();
    let mut transaction = state.begin_transaction();
    transaction
        .apply(&CreateCorridor::new(corridor(3.0)))
        .unwrap();
    let error = transaction
        .apply(&CreateCorridor::new(corridor(3.0)))
        .unwrap_err();
    assert_eq!(error.code(), CommandErrorCode::CorridorAlreadyExists);
    assert_eq!(
        transaction.commit(&mut state).unwrap_err().code(),
        CommandErrorCode::TransactionAborted
    );
    assert_eq!(state, before);
}

#[test]
fn stale_and_empty_transactions_cannot_publish() {
    let mut state = ModelState::new(empty_project());
    let empty = state.begin_transaction();
    assert_eq!(
        empty.commit(&mut state).unwrap_err().code(),
        CommandErrorCode::EmptyTransaction
    );

    let mut stale = state.begin_transaction();
    stale.apply(&CreateCorridor::new(corridor(3.0))).unwrap();
    state.execute(&CreateCorridor::new(corridor(4.0))).unwrap();
    let before = state.clone();
    assert_eq!(
        stale.commit(&mut state).unwrap_err().code(),
        CommandErrorCode::StaleRevision
    );
    assert_eq!(state, before);
}

#[test]
fn transaction_cannot_publish_into_an_unrelated_state_at_the_same_revision() {
    let source = ModelState::new(empty_project());
    let mut transaction = source.begin_transaction();
    transaction
        .apply(&CreateCorridor::new(corridor(3.0)))
        .unwrap();

    let mut unrelated = ModelState::new(empty_project());
    let before = unrelated.clone();
    let error = transaction.commit(&mut unrelated).unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::WrongState);
    assert_eq!(unrelated, before);
}

#[test]
fn transaction_may_temporarily_violate_catalog_invariants_if_final_state_is_valid() {
    let project = project_with_corridors(vec![
        corridor_with_ids(10, 20, 3.0),
        corridor_with_ids(11, 21, 3.0),
    ]);
    let mut state = ModelState::new(project);
    let mut transaction = state.begin_transaction();

    transaction
        .apply(&UpdateCorridor::new(corridor_with_ids(10, 21, 3.0)))
        .unwrap();
    transaction
        .apply(&UpdateCorridor::new(corridor_with_ids(11, 20, 3.0)))
        .unwrap();
    transaction.commit(&mut state).unwrap();

    assert_eq!(state.revision().get(), 1);
    let first = state
        .project()
        .design_catalog()
        .corridor(CorridorId::from_u128(10))
        .unwrap();
    let second = state
        .project()
        .design_catalog()
        .corridor(CorridorId::from_u128(11))
        .unwrap();
    assert_eq!(first.lane_definitions()[0].id(), LaneId::from_u128(21));
    assert_eq!(second.lane_definitions()[0].id(), LaneId::from_u128(20));
}

#[test]
fn invalid_final_catalog_is_rejected_without_publishing() {
    let project = project_with_corridors(vec![
        corridor_with_ids(10, 20, 3.0),
        corridor_with_ids(11, 21, 3.0),
    ]);
    let mut state = ModelState::new(project);
    let before = state.clone();
    let mut transaction = state.begin_transaction();
    transaction
        .apply(&UpdateCorridor::new(corridor_with_ids(10, 21, 3.0)))
        .unwrap();

    let error = transaction.commit(&mut state).unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::DomainInvariant);
    assert_eq!(error.object_refs().len(), 2);
    assert_eq!(state, before);
}

#[test]
fn undo_and_redo_restore_corridor_state_and_stable_ids() {
    let mut state = ModelState::new(empty_project());
    let original = state.project().clone();
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(8).unwrap());

    history
        .execute(&mut state, &CreateCorridor::new(corridor(3.0)))
        .unwrap();
    let created = state.project().clone();
    assert_eq!(history.undo_len(), 1);

    history.undo(&mut state).unwrap();
    assert_eq!(state.project(), &original);
    assert_eq!(state.revision().get(), 2);
    assert_eq!(history.redo_len(), 1);

    history.redo(&mut state).unwrap();
    assert_eq!(state.project(), &created);
    assert_eq!(state.revision().get(), 3);
    assert!(
        state
            .project()
            .design_catalog()
            .corridor(CorridorId::from_u128(10))
            .is_some()
    );
}

#[test]
fn multi_command_transaction_is_one_undoable_history_entry() {
    let mut state = ModelState::new(empty_project());
    let original = state.project().clone();
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(8).unwrap());
    let mut transaction = state.begin_transaction();
    transaction
        .apply(&CreateCorridor::new(corridor(3.0)))
        .unwrap();
    transaction
        .apply(&UpdateCorridor::new(corridor(4.0)))
        .unwrap();

    let outcome = history.commit_transaction(&mut state, transaction).unwrap();

    assert_eq!(outcome.inverse().operation_count(), 2);
    assert_eq!(history.undo_len(), 1);
    history.undo(&mut state).unwrap();
    assert_eq!(state.project(), &original);
    assert_eq!(history.undo_len(), 0);
}

#[test]
fn bounded_history_evicts_oldest_entry_and_new_command_clears_redo() {
    let mut state = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(2).unwrap());
    history
        .execute(&mut state, &CreateCorridor::new(corridor(2.0)))
        .unwrap();
    history
        .execute(&mut state, &UpdateCorridor::new(corridor(3.0)))
        .unwrap();
    history
        .execute(&mut state, &UpdateCorridor::new(corridor(4.0)))
        .unwrap();
    assert_eq!(history.undo_len(), 2);

    history.undo(&mut state).unwrap();
    history.undo(&mut state).unwrap();
    assert_eq!(
        history.undo(&mut state).unwrap_err().code(),
        CommandErrorCode::UndoUnavailable
    );
    assert_eq!(history.redo_len(), 2);

    history.redo(&mut state).unwrap();
    history
        .execute(&mut state, &UpdateCorridor::new(corridor(5.0)))
        .unwrap();
    assert_eq!(history.redo_len(), 0);
    assert_eq!(
        history.redo(&mut state).unwrap_err().code(),
        CommandErrorCode::RedoUnavailable
    );
}

#[test]
fn history_rejects_an_unrelated_model_lineage() {
    let source = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&source, NonZeroUsize::new(2).unwrap());
    let mut unrelated = ModelState::new(empty_project());
    let before = unrelated.clone();

    let error = history
        .execute(&mut unrelated, &CreateCorridor::new(corridor(3.0)))
        .unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::WrongState);
    assert_eq!(unrelated, before);
    assert_eq!(history.undo_len(), 0);
}

#[test]
fn failed_recorded_command_does_not_change_history_or_state() {
    let mut state = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(2).unwrap());
    history
        .execute(&mut state, &CreateCorridor::new(corridor(3.0)))
        .unwrap();
    let before = state.clone();

    let error = history
        .execute(&mut state, &CreateCorridor::new(corridor(3.0)))
        .unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::CorridorAlreadyExists);
    assert_eq!(state, before);
    assert_eq!(history.undo_len(), 1);
}

#[test]
fn history_rejects_unrecorded_changes_on_the_same_lineage() {
    let mut state = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(2).unwrap());
    history
        .execute(&mut state, &CreateCorridor::new(corridor(3.0)))
        .unwrap();
    state.execute(&UpdateCorridor::new(corridor(4.0))).unwrap();
    let before = state.clone();

    let error = history.undo(&mut state).unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::StaleRevision);
    assert_eq!(state, before);
    assert_eq!(history.undo_len(), 1);
}

#[test]
fn history_records_a_valid_envelope_and_rejects_it_after_it_becomes_stale() {
    let mut state = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&state, NonZeroUsize::new(2).unwrap());
    let first = state.envelope(CreateCorridor::new(corridor(3.0)));
    history.execute_envelope(&mut state, &first).unwrap();
    assert_eq!(history.undo_len(), 1);

    let stale = state.envelope(UpdateCorridor::new(corridor(4.0)));
    history
        .execute(&mut state, &UpdateCorridor::new(corridor(5.0)))
        .unwrap();
    let before = state.clone();
    let undo_len = history.undo_len();
    let redo_len = history.redo_len();

    let error = history.execute_envelope(&mut state, &stale).unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::StaleRevision);
    assert_eq!(state, before);
    assert_eq!(history.undo_len(), undo_len);
    assert_eq!(history.redo_len(), redo_len);
}

#[test]
fn history_rejects_an_envelope_from_another_lineage_without_stack_changes() {
    let source = ModelState::new(empty_project());
    let envelope = source.envelope(CreateCorridor::new(corridor(3.0)));
    let mut target = ModelState::new(empty_project());
    let mut history = CommandHistory::new(&target, NonZeroUsize::new(2).unwrap());
    let before = target.clone();

    let error = history
        .execute_envelope(&mut target, &envelope)
        .unwrap_err();

    assert_eq!(error.code(), CommandErrorCode::WrongState);
    assert_eq!(target, before);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
}

proptest! {
    #[test]
    fn successful_command_batch_publishes_exactly_one_revision(
        initial_width in 0.5_f64..10.0,
        updated_width in 0.5_f64..10.0,
    ) {
        let mut state = ModelState::new(empty_project());
        let mut transaction = state.begin_transaction();
        transaction
            .apply(&CreateCorridor::new(corridor(initial_width)))
            .unwrap();
        transaction
            .apply(&UpdateCorridor::new(corridor(updated_width)))
            .unwrap();

        transaction.commit(&mut state).unwrap();

        prop_assert_eq!(state.revision().get(), 1);
        let stored_width = state
            .project()
            .design_catalog()
            .corridor(CorridorId::from_u128(10))
            .unwrap()
            .cross_section_profile()
            .sections()[0]
            .layout()
            .total_width()
            .get();
        prop_assert_eq!(stored_width, updated_width);
    }

    #[test]
    fn apply_then_undo_restores_the_previous_semantic_project(
        width in 0.5_f64..10.0,
    ) {
        let mut state = ModelState::new(empty_project());
        let previous = state.project().clone();
        let mut history = CommandHistory::new(&state, NonZeroUsize::new(1).unwrap());

        history
            .execute(&mut state, &CreateCorridor::new(corridor(width)))
            .unwrap();
        history.undo(&mut state).unwrap();

        prop_assert_eq!(state.project(), &previous);
    }
}
