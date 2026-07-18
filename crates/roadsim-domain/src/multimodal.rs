use crate::{Point2Meters, ReferenceLine};
use roadsim_types::{
    CorridorId, CrossingId, JunctionId, LengthMeters, ObjectRef, RailAlignmentId, SidewalkId,
    WalkingAreaId,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultimodalErrorCode {
    TooFewJunctionApproaches,
    DuplicateJunctionApproach,
    TooFewWalkingAreaVertices,
    DuplicateWalkingAreaVertex,
    InvalidStationRange,
    ZeroWidth,
    SameCrossingEndpoint,
}

impl MultimodalErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooFewJunctionApproaches => "domain.junction.approaches.too_few",
            Self::DuplicateJunctionApproach => "domain.junction.approach.duplicate",
            Self::TooFewWalkingAreaVertices => "domain.walking_area.vertices.too_few",
            Self::DuplicateWalkingAreaVertex => "domain.walking_area.vertex.duplicate",
            Self::InvalidStationRange => "domain.sidewalk.station_range.invalid",
            Self::ZeroWidth => "domain.multimodal.width.zero",
            Self::SameCrossingEndpoint => "domain.crossing.endpoints.same",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultimodalError {
    code: MultimodalErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl MultimodalError {
    fn new(code: MultimodalErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }
    #[must_use]
    pub const fn code(&self) -> MultimodalErrorCode {
        self.code
    }
    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for MultimodalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}
impl Error for MultimodalError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorridorEnd {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CorridorEndpointRef {
    corridor_id: CorridorId,
    end: CorridorEnd,
}

impl CorridorEndpointRef {
    #[must_use]
    pub const fn new(corridor_id: CorridorId, end: CorridorEnd) -> Self {
        Self { corridor_id, end }
    }
    #[must_use]
    pub const fn corridor_id(self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn end(self) -> CorridorEnd {
        self.end
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Junction {
    id: JunctionId,
    approaches: Vec<CorridorEndpointRef>,
}

impl Junction {
    pub fn new(
        id: JunctionId,
        mut approaches: Vec<CorridorEndpointRef>,
    ) -> Result<Self, MultimodalError> {
        if approaches.len() < 2 {
            return Err(MultimodalError::new(
                MultimodalErrorCode::TooFewJunctionApproaches,
                vec![id.into()],
            ));
        }
        approaches.sort_unstable();
        if approaches.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(MultimodalError::new(
                MultimodalErrorCode::DuplicateJunctionApproach,
                vec![id.into(), approaches[0].corridor_id().into()],
            ));
        }
        Ok(Self { id, approaches })
    }
    #[must_use]
    pub const fn id(&self) -> JunctionId {
        self.id
    }
    #[must_use]
    pub fn approaches(&self) -> &[CorridorEndpointRef] {
        &self.approaches
    }
}

impl<'de> Deserialize<'de> for Junction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: JunctionId,
            approaches: Vec<CorridorEndpointRef>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.approaches).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WalkingArea {
    id: WalkingAreaId,
    boundary: Vec<Point2Meters>,
}

impl WalkingArea {
    pub fn new(id: WalkingAreaId, boundary: Vec<Point2Meters>) -> Result<Self, MultimodalError> {
        if boundary.len() < 3 {
            return Err(MultimodalError::new(
                MultimodalErrorCode::TooFewWalkingAreaVertices,
                vec![id.into()],
            ));
        }
        let has_duplicate = boundary
            .iter()
            .enumerate()
            .any(|(index, point)| boundary[index + 1..].contains(point));
        if has_duplicate {
            return Err(MultimodalError::new(
                MultimodalErrorCode::DuplicateWalkingAreaVertex,
                vec![id.into()],
            ));
        }
        Ok(Self { id, boundary })
    }
    #[must_use]
    pub const fn id(&self) -> WalkingAreaId {
        self.id
    }
    #[must_use]
    pub fn boundary(&self) -> &[Point2Meters] {
        &self.boundary
    }
}

impl<'de> Deserialize<'de> for WalkingArea {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: WalkingAreaId,
            boundary: Vec<Point2Meters>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.boundary).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorridorSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sidewalk {
    id: SidewalkId,
    corridor_id: CorridorId,
    side: CorridorSide,
    start_station_m: LengthMeters,
    end_station_m: LengthMeters,
    width_m: LengthMeters,
}

impl Sidewalk {
    pub fn new(
        id: SidewalkId,
        corridor_id: CorridorId,
        side: CorridorSide,
        start_station_m: LengthMeters,
        end_station_m: LengthMeters,
        width_m: LengthMeters,
    ) -> Result<Self, MultimodalError> {
        if end_station_m.get() <= start_station_m.get() {
            return Err(MultimodalError::new(
                MultimodalErrorCode::InvalidStationRange,
                vec![id.into(), corridor_id.into()],
            ));
        }
        if width_m.get() == 0.0 {
            return Err(MultimodalError::new(
                MultimodalErrorCode::ZeroWidth,
                vec![id.into(), corridor_id.into()],
            ));
        }
        Ok(Self {
            id,
            corridor_id,
            side,
            start_station_m,
            end_station_m,
            width_m,
        })
    }
    #[must_use]
    pub const fn id(&self) -> SidewalkId {
        self.id
    }
    #[must_use]
    pub const fn corridor_id(&self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn side(&self) -> CorridorSide {
        self.side
    }
    #[must_use]
    pub const fn start_station(&self) -> LengthMeters {
        self.start_station_m
    }
    #[must_use]
    pub const fn end_station(&self) -> LengthMeters {
        self.end_station_m
    }
    #[must_use]
    pub const fn width(&self) -> LengthMeters {
        self.width_m
    }
}

impl<'de> Deserialize<'de> for Sidewalk {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: SidewalkId,
            corridor_id: CorridorId,
            side: CorridorSide,
            start_station_m: LengthMeters,
            end_station_m: LengthMeters,
            width_m: LengthMeters,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(
            w.id,
            w.corridor_id,
            w.side,
            w.start_station_m,
            w.end_station_m,
            w.width_m,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Crossing {
    id: CrossingId,
    corridor_id: CorridorId,
    station_m: LengthMeters,
    width_m: LengthMeters,
    from: WalkingAreaId,
    to: WalkingAreaId,
}

impl Crossing {
    pub fn new(
        id: CrossingId,
        corridor_id: CorridorId,
        station_m: LengthMeters,
        width_m: LengthMeters,
        from: WalkingAreaId,
        to: WalkingAreaId,
    ) -> Result<Self, MultimodalError> {
        if width_m.get() == 0.0 {
            return Err(MultimodalError::new(
                MultimodalErrorCode::ZeroWidth,
                vec![id.into(), corridor_id.into()],
            ));
        }
        if from == to {
            return Err(MultimodalError::new(
                MultimodalErrorCode::SameCrossingEndpoint,
                vec![id.into(), from.into()],
            ));
        }
        Ok(Self {
            id,
            corridor_id,
            station_m,
            width_m,
            from,
            to,
        })
    }
    #[must_use]
    pub const fn id(&self) -> CrossingId {
        self.id
    }
    #[must_use]
    pub const fn corridor_id(&self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn station(&self) -> LengthMeters {
        self.station_m
    }
    #[must_use]
    pub const fn width(&self) -> LengthMeters {
        self.width_m
    }
    #[must_use]
    pub const fn from(&self) -> WalkingAreaId {
        self.from
    }
    #[must_use]
    pub const fn to(&self) -> WalkingAreaId {
        self.to
    }
}

impl<'de> Deserialize<'de> for Crossing {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: CrossingId,
            corridor_id: CorridorId,
            station_m: LengthMeters,
            width_m: LengthMeters,
            from: WalkingAreaId,
            to: WalkingAreaId,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(w.id, w.corridor_id, w.station_m, w.width_m, w.from, w.to)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RailAlignment {
    id: RailAlignmentId,
    reference_line: ReferenceLine,
    track_gauge_m: LengthMeters,
}

impl RailAlignment {
    pub fn new(
        id: RailAlignmentId,
        reference_line: ReferenceLine,
        track_gauge_m: LengthMeters,
    ) -> Result<Self, MultimodalError> {
        if track_gauge_m.get() == 0.0 {
            return Err(MultimodalError::new(
                MultimodalErrorCode::ZeroWidth,
                vec![id.into()],
            ));
        }
        Ok(Self {
            id,
            reference_line,
            track_gauge_m,
        })
    }
    #[must_use]
    pub const fn id(&self) -> RailAlignmentId {
        self.id
    }
    #[must_use]
    pub const fn reference_line(&self) -> &ReferenceLine {
        &self.reference_line
    }
    #[must_use]
    pub const fn track_gauge(&self) -> LengthMeters {
        self.track_gauge_m
    }
}

impl<'de> Deserialize<'de> for RailAlignment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: RailAlignmentId,
            reference_line: ReferenceLine,
            track_gauge_m: LengthMeters,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(w.id, w.reference_line, w.track_gauge_m).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogErrorCode {
    CorridorInvariant,
    DuplicateEntity,
    DanglingCorridor,
    DanglingLane,
    DanglingJunction,
    DanglingWalkingArea,
    DanglingSignalGroup,
    DanglingSignalProgram,
    SignalJunctionMismatch,
    EndpointAlreadyConnected,
    StationOutsideCorridor,
}

impl CatalogErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CorridorInvariant => "domain.catalog.corridor_invariant",
            Self::DuplicateEntity => "domain.catalog.entity.duplicate",
            Self::DanglingCorridor => "domain.catalog.corridor_ref.dangling",
            Self::DanglingLane => "domain.catalog.lane_ref.dangling",
            Self::DanglingJunction => "domain.catalog.junction_ref.dangling",
            Self::DanglingWalkingArea => "domain.catalog.walking_area_ref.dangling",
            Self::DanglingSignalGroup => "domain.catalog.signal_group_ref.dangling",
            Self::DanglingSignalProgram => "domain.catalog.signal_program_ref.dangling",
            Self::SignalJunctionMismatch => "domain.catalog.signal_junction.mismatch",
            Self::EndpointAlreadyConnected => "domain.catalog.corridor_endpoint.duplicate",
            Self::StationOutsideCorridor => "domain.catalog.station.outside_corridor",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogError {
    code: CatalogErrorCode,
    object_refs: Vec<ObjectRef>,
}
impl CatalogError {
    pub(crate) fn new(code: CatalogErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }
    #[must_use]
    pub const fn code(&self) -> CatalogErrorCode {
        self.code
    }
    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}
impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code.as_str())
    }
}
impl Error for CatalogError {}

pub(crate) fn duplicate_object_ref<T>(values: impl IntoIterator<Item = T>) -> Option<ObjectRef>
where
    T: Ord + Copy + Into<ObjectRef>,
{
    let mut ids = BTreeSet::new();
    values
        .into_iter()
        .find_map(|id| (!ids.insert(id)).then(|| id.into()))
}
