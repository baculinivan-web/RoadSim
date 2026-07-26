//! Minimal command-driven editing over the demo Design project.
//!
//! Every user gesture commits exactly one undoable command through
//! `roadsim-commands`; the editor never mutates the project directly and a
//! failed command leaves both the model revision and the last compiled
//! network untouched.

use crate::demo;
use roadsim_commands::{
    CommandHistory, CreateCorridor, DeleteCorridor, ModelState, UpdateCorridor,
};
use roadsim_compiled_network::CompiledNetwork;
use roadsim_domain::{
    Corridor, CrossSectionLayout, CrossSectionProfile, CrossSectionSection, LaneDefinition,
    LaneDirection, LaneUse, Point2Meters, ReferenceLine, ReferenceLineElement, ReferenceLinePose,
};
use roadsim_types::{CoordinateMeters, CorridorId, HeadingRadians, LaneId, LengthMeters};
use std::{num::NonZeroUsize, sync::Arc};

const HISTORY_CAPACITY: usize = 64;
/// Widths a lane editor accepts, in metres. Normative checks belong to E08.
pub const MIN_LANE_WIDTH_M: f64 = 2.5;
pub const MAX_LANE_WIDTH_M: f64 = 5.0;
const ADDED_ROAD_LENGTH_M: f64 = 120.0;
const ADDED_ROAD_SPACING_M: f64 = 15.0;

/// One lane row shown by the inspector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaneRow {
    pub corridor_id: CorridorId,
    pub lane_id: LaneId,
    pub width_m: f64,
}

/// Command-driven editor state plus the network compiled from it.
pub struct EditorState {
    model: ModelState,
    history: CommandHistory,
    network: Arc<CompiledNetwork>,
    /// Y offset for the next added parallel road, so IDs and geometry differ.
    next_road_index: u64,
    error_code: Option<String>,
}

impl EditorState {
    pub fn new() -> Result<Self, String> {
        let model = ModelState::new(demo::project()?);
        let network = Arc::new(demo::compile(model.project())?);
        let history = CommandHistory::new(
            &model,
            NonZeroUsize::new(HISTORY_CAPACITY).expect("capacity is non-zero"),
        );
        Ok(Self {
            model,
            history,
            network,
            next_road_index: 1,
            error_code: None,
        })
    }

    #[must_use]
    pub const fn network(&self) -> &Arc<CompiledNetwork> {
        &self.network
    }

    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.model.revision().get()
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.undo_len() > 0
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.redo_len() > 0
    }

    /// Lane rows of every corridor, in stable catalog order.
    #[must_use]
    pub fn lane_rows(&self) -> Vec<LaneRow> {
        let mut rows = Vec::new();
        for corridor in self.model.project().design_catalog().corridors() {
            for section in corridor.cross_section_profile().sections() {
                let layout = section.layout();
                for slice in layout.left().iter().chain(layout.right()) {
                    rows.push(LaneRow {
                        corridor_id: corridor.id(),
                        lane_id: slice.lane_id(),
                        width_m: slice.width().get(),
                    });
                }
            }
        }
        rows
    }

    #[must_use]
    pub fn corridor_ids(&self) -> Vec<CorridorId> {
        self.model
            .project()
            .design_catalog()
            .corridors()
            .iter()
            .map(Corridor::id)
            .collect()
    }

    /// One gesture: set the width of one lane and recompile.
    pub fn set_lane_width(&mut self, corridor_id: CorridorId, lane_id: LaneId, width_m: f64) {
        if !(MIN_LANE_WIDTH_M..=MAX_LANE_WIDTH_M).contains(&width_m) {
            self.error_code = Some("editor.lane_width.out_of_range".to_owned());
            return;
        }
        let Some(previous) = self
            .model
            .project()
            .design_catalog()
            .corridors()
            .iter()
            .find(|corridor| corridor.id() == corridor_id)
            .cloned()
        else {
            self.error_code = Some("editor.corridor.not_found".to_owned());
            return;
        };
        match rebuild_with_lane_width(&previous, lane_id, width_m) {
            Ok(updated) => self.commit(UpdateCorridor::new(updated)),
            Err(code) => self.error_code = Some(code),
        }
    }

    /// One gesture: add a parallel straight road below the existing ones.
    pub fn add_parallel_road(&mut self) {
        let index = self.next_road_index;
        match parallel_road(index) {
            Ok(corridor) => {
                self.commit(CreateCorridor::new(corridor));
                if self.error_code.is_none() {
                    self.next_road_index += 1;
                }
            }
            Err(code) => self.error_code = Some(code),
        }
    }

    /// One gesture: delete one corridor. The last road cannot be deleted
    /// because an empty network no longer compiles.
    pub fn delete_corridor(&mut self, corridor_id: CorridorId) {
        if self.corridor_ids().len() <= 1 {
            self.error_code = Some("editor.corridor.last_road".to_owned());
            return;
        }
        self.commit(DeleteCorridor::new(corridor_id));
    }

    pub fn undo(&mut self) {
        self.error_code = None;
        if let Err(error) = self.history.undo(&mut self.model) {
            self.error_code = Some(error.code().as_str().to_owned());
            return;
        }
        self.recompile();
    }

    pub fn redo(&mut self) {
        self.error_code = None;
        if let Err(error) = self.history.redo(&mut self.model) {
            self.error_code = Some(error.code().as_str().to_owned());
            return;
        }
        self.recompile();
    }

    fn commit<C: roadsim_commands::Command>(&mut self, command: C) {
        self.error_code = None;
        if let Err(error) = self.history.execute(&mut self.model, &command) {
            self.error_code = Some(error.code().as_str().to_owned());
            return;
        }
        self.recompile();
    }

    /// Compiles the edited project; on failure the previous network stays
    /// current and the compiler diagnostic is surfaced instead.
    fn recompile(&mut self) {
        match demo::compile(self.model.project()) {
            Ok(network) => self.network = Arc::new(network),
            Err(code) => self.error_code = Some(code),
        }
    }
}

fn rebuild_with_lane_width(
    corridor: &Corridor,
    lane_id: LaneId,
    width_m: f64,
) -> Result<Corridor, String> {
    let width = LengthMeters::try_new(width_m).map_err(|error| error.to_string())?;
    let mut found = false;
    let mut sections = Vec::new();
    for section in corridor.cross_section_profile().sections() {
        let layout = section.layout();
        let rebuild_side = |slices: &[roadsim_domain::LaneSlice]| {
            slices
                .iter()
                .map(|slice| {
                    let slice_width = if slice.lane_id() == lane_id {
                        width
                    } else {
                        slice.width()
                    };
                    roadsim_domain::LaneSlice::new(slice.lane_id(), slice_width)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, String>>()
        };
        found |= layout
            .left()
            .iter()
            .chain(layout.right())
            .any(|slice| slice.lane_id() == lane_id);
        sections.push(CrossSectionSection::new(
            section.start_station(),
            CrossSectionLayout::new(rebuild_side(layout.left())?, rebuild_side(layout.right())?)
                .map_err(|error| error.to_string())?,
        ));
    }
    if !found {
        return Err("editor.lane.not_found".to_owned());
    }
    Corridor::new(
        corridor.id(),
        corridor.reference_line().clone(),
        corridor.lane_definitions().to_vec(),
        CrossSectionProfile::new(sections).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

/// Deterministic new straight road: same length, offset south per index.
fn parallel_road(index: u64) -> Result<Corridor, String> {
    let corridor_id = CorridorId::from_u128(0x200 + u128::from(index));
    let left_lane_id = LaneId::from_u128(0x2000 + u128::from(index) * 2);
    let right_lane_id = LaneId::from_u128(0x2001 + u128::from(index) * 2);
    let length = |value: f64| LengthMeters::try_new(value).map_err(|error| error.to_string());
    let coordinate =
        |value: f64| CoordinateMeters::try_new(value).map_err(|error| error.to_string());
    #[allow(clippy::cast_precision_loss)]
    let offset_y = -(index as f64) * ADDED_ROAD_SPACING_M;

    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(coordinate(-60.0)?, coordinate(offset_y)?),
            HeadingRadians::try_new(0.0).map_err(|error| error.to_string())?,
        ),
        vec![
            ReferenceLineElement::line(length(ADDED_ROAD_LENGTH_M)?)
                .map_err(|error| error.to_string())?,
        ],
    )
    .map_err(|error| error.to_string())?;
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0)?,
        CrossSectionLayout::new(
            vec![
                roadsim_domain::LaneSlice::new(left_lane_id, length(3.5)?)
                    .map_err(|error| error.to_string())?,
            ],
            vec![
                roadsim_domain::LaneSlice::new(right_lane_id, length(3.5)?)
                    .map_err(|error| error.to_string())?,
            ],
        )
        .map_err(|error| error.to_string())?,
    )])
    .map_err(|error| error.to_string())?;
    Corridor::new(
        corridor_id,
        reference_line,
        vec![
            LaneDefinition::new(
                left_lane_id,
                LaneDirection::AgainstReference,
                LaneUse::GeneralTraffic,
            ),
            LaneDefinition::new(
                right_lane_id,
                LaneDirection::AlongReference,
                LaneUse::GeneralTraffic,
            ),
        ],
        profile,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_lane() -> (CorridorId, LaneId) {
        (CorridorId::from_u128(0x100), LaneId::from_u128(0x101))
    }

    #[test]
    fn lane_width_edit_is_one_undoable_command_that_recompiles() {
        let mut editor = EditorState::new().unwrap();
        let baseline = editor.network().header().content_hash();
        let (corridor_id, lane_id) = demo_lane();

        editor.set_lane_width(corridor_id, lane_id, 3.0);
        assert_eq!(editor.error_code(), None);
        assert_eq!(editor.revision(), 1);
        assert!(editor.can_undo());
        let edited = editor.network().header().content_hash();
        assert_ne!(edited, baseline);
        assert!(
            editor
                .lane_rows()
                .iter()
                .any(|row| row.lane_id == lane_id && (row.width_m - 3.0).abs() < 1.0e-12)
        );

        editor.undo();
        assert_eq!(editor.error_code(), None);
        assert_eq!(editor.network().header().content_hash(), baseline);

        editor.redo();
        assert_eq!(editor.network().header().content_hash(), edited);
    }

    #[test]
    fn out_of_range_width_is_refused_without_touching_model_or_network() {
        let mut editor = EditorState::new().unwrap();
        let baseline = editor.network().header().content_hash();
        let (corridor_id, lane_id) = demo_lane();

        editor.set_lane_width(corridor_id, lane_id, 0.5);
        assert_eq!(editor.error_code(), Some("editor.lane_width.out_of_range"));
        assert_eq!(editor.revision(), 0);
        assert!(!editor.can_undo());
        assert_eq!(editor.network().header().content_hash(), baseline);
    }

    #[test]
    fn adding_and_deleting_a_parallel_road_round_trips_the_network() {
        let mut editor = EditorState::new().unwrap();
        let baseline = editor.network().header().content_hash();
        assert_eq!(editor.network().lanes().len(), 2);

        editor.add_parallel_road();
        assert_eq!(editor.error_code(), None);
        assert_eq!(editor.network().lanes().len(), 4);
        assert_eq!(editor.corridor_ids().len(), 2);

        let added = editor.corridor_ids()[1];
        editor.delete_corridor(added);
        assert_eq!(editor.error_code(), None);
        assert_eq!(editor.network().lanes().len(), 2);
        assert_eq!(editor.network().header().content_hash(), baseline);
    }

    #[test]
    fn the_last_road_cannot_be_deleted_because_empty_networks_do_not_compile() {
        let mut editor = EditorState::new().unwrap();
        let only = editor.corridor_ids()[0];
        editor.delete_corridor(only);
        assert_eq!(editor.error_code(), Some("editor.corridor.last_road"));
        assert_eq!(editor.corridor_ids().len(), 1);
    }

    #[test]
    fn unknown_targets_are_diagnosed_without_a_history_entry() {
        let mut editor = EditorState::new().unwrap();
        editor.set_lane_width(CorridorId::from_u128(0xdead), LaneId::from_u128(0x101), 3.0);
        assert_eq!(editor.error_code(), Some("editor.corridor.not_found"));
        let (corridor_id, _) = demo_lane();
        editor.set_lane_width(corridor_id, LaneId::from_u128(0xdead), 3.0);
        assert_eq!(editor.error_code(), Some("editor.lane.not_found"));
        assert!(!editor.can_undo());
    }
}
