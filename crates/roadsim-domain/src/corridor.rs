use crate::{ReferenceLine, StationRange};
use roadsim_types::{CorridorId, LaneId, LengthMeters};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

/// Stable validation failures for corridor and cross-section authoring data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorridorErrorCode {
    EmptyLaneCatalog,
    DuplicateLaneDefinition,
    EmptyCrossSectionProfile,
    FirstSectionNotAtZero,
    SectionStationNotIncreasing,
    SectionOutsideReferenceLine,
    EmptyLaneLayout,
    ZeroLaneWidth,
    DuplicateLaneReference,
    DanglingLaneReference,
    UnusedLaneDefinition,
    CrossSectionWidthOverflow,
    CrossSectionWidthResolutionLost,
    DuplicateCorridor,
    DuplicateProjectLaneId,
}

impl CorridorErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyLaneCatalog => "domain.corridor.lanes.empty",
            Self::DuplicateLaneDefinition => "domain.corridor.lanes.duplicate_definition",
            Self::EmptyCrossSectionProfile => "domain.corridor.profile.empty",
            Self::FirstSectionNotAtZero => "domain.corridor.profile.first_section_not_zero",
            Self::SectionStationNotIncreasing => "domain.corridor.profile.station_not_increasing",
            Self::SectionOutsideReferenceLine => {
                "domain.corridor.profile.section_outside_reference_line"
            }
            Self::EmptyLaneLayout => "domain.corridor.section.lanes.empty",
            Self::ZeroLaneWidth => "domain.corridor.lane.zero_width",
            Self::DuplicateLaneReference => "domain.corridor.section.lane_ref.duplicate",
            Self::DanglingLaneReference => "domain.corridor.section.lane_ref.dangling",
            Self::UnusedLaneDefinition => "domain.corridor.lane.unused",
            Self::CrossSectionWidthOverflow => "domain.corridor.section.width_overflow",
            Self::CrossSectionWidthResolutionLost => {
                "domain.corridor.section.width_resolution_lost"
            }
            Self::DuplicateCorridor => "domain.catalog.corridor.duplicate",
            Self::DuplicateProjectLaneId => "domain.catalog.lane_id.duplicate",
        }
    }
}

/// Corridor validation evidence suitable for conversion to a public diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CorridorError {
    code: CorridorErrorCode,
    corridor_id: Option<CorridorId>,
    section_index: Option<usize>,
    lane_id: Option<LaneId>,
}

impl CorridorError {
    const fn new(code: CorridorErrorCode) -> Self {
        Self {
            code,
            corridor_id: None,
            section_index: None,
            lane_id: None,
        }
    }

    const fn at_section(mut self, index: usize) -> Self {
        self.section_index = Some(index);
        self
    }

    const fn for_lane(mut self, id: LaneId) -> Self {
        self.lane_id = Some(id);
        self
    }

    const fn for_corridor(mut self, id: CorridorId) -> Self {
        self.corridor_id = Some(id);
        self
    }

    #[must_use]
    pub const fn code(self) -> CorridorErrorCode {
        self.code
    }

    #[must_use]
    pub const fn corridor_id(self) -> Option<CorridorId> {
        self.corridor_id
    }

    #[must_use]
    pub const fn section_index(self) -> Option<usize> {
        self.section_index
    }

    #[must_use]
    pub const fn lane_id(self) -> Option<LaneId> {
        self.lane_id
    }
}

impl fmt::Display for CorridorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.code.as_str())?;
        if let Some(id) = self.corridor_id {
            write!(formatter, " corridor={id}")?;
        }
        if let Some(index) = self.section_index {
            write!(formatter, " section={index}")?;
        }
        if let Some(id) = self.lane_id {
            write!(formatter, " lane={id}")?;
        }
        Ok(())
    }
}

impl Error for CorridorError {}

/// Travel direction relative to increasing reference-line station.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneDirection {
    AlongReference,
    AgainstReference,
}

/// Backend-independent semantic use of a road lane.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneUse {
    GeneralTraffic,
    BusOnly,
    Bicycle,
    Parking,
}

/// Corridor-local, single-source lane semantics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaneDefinition {
    id: LaneId,
    direction: LaneDirection,
    use_kind: LaneUse,
}

impl LaneDefinition {
    #[must_use]
    pub const fn new(id: LaneId, direction: LaneDirection, use_kind: LaneUse) -> Self {
        Self {
            id,
            direction,
            use_kind,
        }
    }
    #[must_use]
    pub const fn id(self) -> LaneId {
        self.id
    }
    #[must_use]
    pub const fn direction(self) -> LaneDirection {
        self.direction
    }
    #[must_use]
    pub const fn use_kind(self) -> LaneUse {
        self.use_kind
    }
}

/// A lane reference and its piecewise-constant width within one profile section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LaneSlice {
    lane_id: LaneId,
    width_m: LengthMeters,
}

impl LaneSlice {
    pub fn new(lane_id: LaneId, width_m: LengthMeters) -> Result<Self, CorridorError> {
        if width_m.get() == 0.0 {
            return Err(CorridorError::new(CorridorErrorCode::ZeroLaneWidth).for_lane(lane_id));
        }
        Ok(Self { lane_id, width_m })
    }
    #[must_use]
    pub const fn lane_id(self) -> LaneId {
        self.lane_id
    }
    #[must_use]
    pub const fn width(self) -> LengthMeters {
        self.width_m
    }
}

impl<'de> Deserialize<'de> for LaneSlice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            lane_id: LaneId,
            width_m: LengthMeters,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.lane_id, wire.width_m).map_err(serde::de::Error::custom)
    }
}

/// Ordered lane layout at a cross-section station.
///
/// Both sides are ordered from the reference line outwards while viewed along
/// increasing station. This lateral convention is independent of lane travel direction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CrossSectionLayout {
    left: Vec<LaneSlice>,
    right: Vec<LaneSlice>,
}

impl CrossSectionLayout {
    pub fn new(left: Vec<LaneSlice>, right: Vec<LaneSlice>) -> Result<Self, CorridorError> {
        if left.is_empty() && right.is_empty() {
            return Err(CorridorError::new(CorridorErrorCode::EmptyLaneLayout));
        }
        let mut ids = BTreeSet::new();
        let mut width = 0.0;
        for lane in left.iter().chain(&right).copied() {
            if !ids.insert(lane.lane_id()) {
                return Err(
                    CorridorError::new(CorridorErrorCode::DuplicateLaneReference)
                        .for_lane(lane.lane_id()),
                );
            }
            let next = width + lane.width().get();
            if !next.is_finite() {
                return Err(
                    CorridorError::new(CorridorErrorCode::CrossSectionWidthOverflow)
                        .for_lane(lane.lane_id()),
                );
            }
            if next <= width {
                return Err(
                    CorridorError::new(CorridorErrorCode::CrossSectionWidthResolutionLost)
                        .for_lane(lane.lane_id()),
                );
            }
            width = next;
        }
        Ok(Self { left, right })
    }
    #[must_use]
    pub fn left(&self) -> &[LaneSlice] {
        &self.left
    }
    #[must_use]
    pub fn right(&self) -> &[LaneSlice] {
        &self.right
    }
    #[must_use]
    pub fn total_width(&self) -> LengthMeters {
        let value = self
            .left
            .iter()
            .chain(&self.right)
            .map(|lane| lane.width().get())
            .sum();
        LengthMeters::try_new(value).expect("layout construction validates total width")
    }
}

impl<'de> Deserialize<'de> for CrossSectionLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            left: Vec<LaneSlice>,
            right: Vec<LaneSlice>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.left, wire.right).map_err(serde::de::Error::custom)
    }
}

/// One piecewise-constant cross-section beginning at `start_station`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CrossSectionSection {
    start_station: LengthMeters,
    layout: CrossSectionLayout,
}

impl CrossSectionSection {
    #[must_use]
    pub const fn new(start_station: LengthMeters, layout: CrossSectionLayout) -> Self {
        Self {
            start_station,
            layout,
        }
    }
    #[must_use]
    pub const fn start_station(&self) -> LengthMeters {
        self.start_station
    }
    #[must_use]
    pub const fn layout(&self) -> &CrossSectionLayout {
        &self.layout
    }
}

/// A non-empty, station-ordered piecewise-constant cross-section profile.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CrossSectionProfile(Vec<CrossSectionSection>);

impl CrossSectionProfile {
    pub fn new(sections: Vec<CrossSectionSection>) -> Result<Self, CorridorError> {
        if sections.is_empty() {
            return Err(CorridorError::new(
                CorridorErrorCode::EmptyCrossSectionProfile,
            ));
        }
        if sections[0].start_station().get() != 0.0 {
            return Err(CorridorError::new(CorridorErrorCode::FirstSectionNotAtZero).at_section(0));
        }
        for (index, pair) in sections.windows(2).enumerate() {
            if pair[1].start_station().get() <= pair[0].start_station().get() {
                return Err(
                    CorridorError::new(CorridorErrorCode::SectionStationNotIncreasing)
                        .at_section(index + 1),
                );
            }
        }
        Ok(Self(sections))
    }
    #[must_use]
    pub fn sections(&self) -> &[CrossSectionSection] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CrossSectionProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let sections = Vec::<CrossSectionSection>::deserialize(deserializer)?;
        Self::new(sections).map_err(serde::de::Error::custom)
    }
}

/// A backend-independent road corridor in the Design Model.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Corridor {
    id: CorridorId,
    reference_line: ReferenceLine,
    lane_definitions: Vec<LaneDefinition>,
    cross_section_profile: CrossSectionProfile,
}

impl Corridor {
    pub fn new(
        id: CorridorId,
        reference_line: ReferenceLine,
        mut lane_definitions: Vec<LaneDefinition>,
        cross_section_profile: CrossSectionProfile,
    ) -> Result<Self, CorridorError> {
        if lane_definitions.is_empty() {
            return Err(CorridorError::new(CorridorErrorCode::EmptyLaneCatalog).for_corridor(id));
        }
        lane_definitions.sort_by_key(|lane| lane.id());
        for pair in lane_definitions.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(
                    CorridorError::new(CorridorErrorCode::DuplicateLaneDefinition)
                        .for_corridor(id)
                        .for_lane(pair[0].id()),
                );
            }
        }
        let defined: BTreeSet<_> = lane_definitions.iter().map(|lane| lane.id()).collect();
        let mut used = BTreeSet::new();
        for (section_index, section) in cross_section_profile.sections().iter().enumerate() {
            if section.start_station().get() >= reference_line.total_length().get() {
                return Err(
                    CorridorError::new(CorridorErrorCode::SectionOutsideReferenceLine)
                        .for_corridor(id)
                        .at_section(section_index),
                );
            }
            for slice in section
                .layout()
                .left()
                .iter()
                .chain(section.layout().right())
            {
                if !defined.contains(&slice.lane_id()) {
                    return Err(CorridorError::new(CorridorErrorCode::DanglingLaneReference)
                        .for_corridor(id)
                        .at_section(section_index)
                        .for_lane(slice.lane_id()));
                }
                used.insert(slice.lane_id());
            }
        }
        if let Some(unused) = defined.difference(&used).next().copied() {
            return Err(CorridorError::new(CorridorErrorCode::UnusedLaneDefinition)
                .for_corridor(id)
                .for_lane(unused));
        }
        Ok(Self {
            id,
            reference_line,
            lane_definitions,
            cross_section_profile,
        })
    }
    #[must_use]
    pub const fn id(&self) -> CorridorId {
        self.id
    }
    #[must_use]
    pub const fn reference_line(&self) -> &ReferenceLine {
        &self.reference_line
    }
    #[must_use]
    pub fn lane_definitions(&self) -> &[LaneDefinition] {
        &self.lane_definitions
    }
    #[must_use]
    pub const fn cross_section_profile(&self) -> &CrossSectionProfile {
        &self.cross_section_profile
    }
    #[must_use]
    pub fn lane(&self, id: LaneId) -> Option<&LaneDefinition> {
        self.lane_definitions
            .binary_search_by_key(&id, |lane| lane.id())
            .ok()
            .map(|index| &self.lane_definitions[index])
    }
    #[must_use]
    pub fn section_ranges(&self) -> Vec<StationRange> {
        let sections = self.cross_section_profile.sections();
        sections
            .iter()
            .enumerate()
            .map(|(index, section)| {
                let end = sections
                    .get(index + 1)
                    .map_or(self.reference_line.total_length(), |next| {
                        next.start_station()
                    });
                StationRange::new(section.start_station(), end)
            })
            .collect()
    }
}

impl<'de> Deserialize<'de> for Corridor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            id: CorridorId,
            reference_line: ReferenceLine,
            lane_definitions: Vec<LaneDefinition>,
            cross_section_profile: CrossSectionProfile,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.reference_line,
            wire.lane_definitions,
            wire.cross_section_profile,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Deterministically ordered editable spatial objects currently supported by E01.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DesignCatalog {
    corridors: Vec<Corridor>,
}

impl DesignCatalog {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            corridors: Vec::new(),
        }
    }
    pub fn new(mut corridors: Vec<Corridor>) -> Result<Self, CorridorError> {
        corridors.sort_by_key(|corridor| corridor.id());
        for pair in corridors.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(CorridorError::new(CorridorErrorCode::DuplicateCorridor)
                    .for_corridor(pair[0].id()));
            }
        }
        let mut project_lane_ids = BTreeSet::new();
        for corridor in &corridors {
            for lane in corridor.lane_definitions() {
                if !project_lane_ids.insert(lane.id()) {
                    return Err(
                        CorridorError::new(CorridorErrorCode::DuplicateProjectLaneId)
                            .for_corridor(corridor.id())
                            .for_lane(lane.id()),
                    );
                }
            }
        }
        Ok(Self { corridors })
    }
    #[must_use]
    pub fn corridors(&self) -> &[Corridor] {
        &self.corridors
    }
    #[must_use]
    pub fn corridor(&self, id: CorridorId) -> Option<&Corridor> {
        self.corridors
            .binary_search_by_key(&id, |corridor| corridor.id())
            .ok()
            .map(|index| &self.corridors[index])
    }
}

impl<'de> Deserialize<'de> for DesignCatalog {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            corridors: Vec<Corridor>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.corridors).map_err(serde::de::Error::custom)
    }
}
