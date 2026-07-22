//! Immutable, backend-independent Compiled Simulation Network contracts.
//!
//! These types intentionally contain neither mutable Design Model objects nor
//! UI/backend implementation types. A compiler produces a complete value and
//! only then publishes it to a backend through `Arc<CompiledNetwork>`.

use roadsim_types::{
    CorridorId, CrossingId, JunctionId, LaneId, Sha256Digest, SidewalkId, WalkingAreaId,
};
use serde::Serialize;
use std::{collections::BTreeSet, error::Error, fmt};

/// Schema of the in-memory CSN contract.
pub const COMPILED_NETWORK_SCHEMA_VERSION: u32 = 4;
pub const MAX_GRAPH_NODES: u32 = 1_000_000;
pub const MAX_GRAPH_EDGES: usize = 4_000_000;

/// Monotonic source model revision captured when compilation starts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourceRevision(u64);

impl SourceRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Coordinate frame used by all compiled geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateFrame {
    /// Project-local Cartesian engineering coordinates in metres.
    LocalMetric,
}

/// Stable capability IDs required by a compiled network before a run starts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CapabilityId {
    #[serde(rename = "road.vehicles.basic")]
    RoadVehiclesBasic,
    #[serde(rename = "transit.bus_lanes")]
    TransitBusLanes,
    #[serde(rename = "road.bicycle_lanes")]
    BicycleLanes,
    #[serde(rename = "road.parking_lanes")]
    ParkingLanes,
    #[serde(rename = "pedestrian.walking_areas.basic")]
    PedestrianWalkingAreasBasic,
}

impl CapabilityId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RoadVehiclesBasic => "road.vehicles.basic",
            Self::TransitBusLanes => "transit.bus_lanes",
            Self::BicycleLanes => "road.bicycle_lanes",
            Self::ParkingLanes => "road.parking_lanes",
            Self::PedestrianWalkingAreasBasic => "pedestrian.walking_areas.basic",
        }
    }
}

/// Deterministically ordered set of capabilities required by the CSN.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CapabilityRequirements(BTreeSet<CapabilityId>);

impl CapabilityRequirements {
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = CapabilityId>) -> Self {
        Self(values.into_iter().collect())
    }

    #[must_use]
    pub const fn values(&self) -> &BTreeSet<CapabilityId> {
        &self.0
    }

    #[must_use]
    pub fn contains(&self, capability: CapabilityId) -> bool {
        self.0.contains(&capability)
    }
}

/// Compact zero-based lane index used by runtime arrays.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CompiledLaneId(u32);

impl CompiledLaneId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One directed coarse adjacency between compiled road lanes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LaneAdjacency {
    from: CompiledLaneId,
    to: CompiledLaneId,
    junction_id: JunctionId,
}

impl LaneAdjacency {
    #[must_use]
    pub const fn new(from: CompiledLaneId, to: CompiledLaneId, junction_id: JunctionId) -> Self {
        Self {
            from,
            to,
            junction_id,
        }
    }

    #[must_use]
    pub const fn from(self) -> CompiledLaneId {
        self.from
    }

    #[must_use]
    pub const fn to(self) -> CompiledLaneId {
        self.to
    }

    #[must_use]
    pub const fn junction_id(self) -> JunctionId {
        self.junction_id
    }
}

/// Compact directed lane reachability graph. Movement geometry is added later.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LaneGraph {
    node_count: u32,
    adjacency: Vec<LaneAdjacency>,
}

impl LaneGraph {
    pub fn new(node_count: u32, mut adjacency: Vec<LaneAdjacency>) -> Result<Self, GraphError> {
        if node_count > MAX_GRAPH_NODES {
            return Err(GraphError(GraphErrorCode::TooManyNodes));
        }
        if adjacency.len() > MAX_GRAPH_EDGES {
            return Err(GraphError(GraphErrorCode::TooManyEdges));
        }
        adjacency.sort_unstable();
        adjacency.dedup();
        if adjacency
            .iter()
            .any(|edge| edge.from().get() >= node_count || edge.to().get() >= node_count)
        {
            return Err(GraphError(GraphErrorCode::NodeOutsideGraph));
        }
        Ok(Self {
            node_count,
            adjacency,
        })
    }

    #[must_use]
    pub const fn node_count(&self) -> u32 {
        self.node_count
    }

    #[must_use]
    pub fn adjacency(&self) -> &[LaneAdjacency] {
        &self.adjacency
    }

    #[must_use]
    pub fn can_reach(&self, from: CompiledLaneId, to: CompiledLaneId) -> bool {
        can_reach(
            self.node_count,
            from.get(),
            to.get(),
            self.adjacency
                .iter()
                .map(|edge| (edge.from().get(), edge.to().get())),
        )
    }
}

/// Compact zero-based movement index used by backend connector tables.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CompiledMovementId(u32);

impl CompiledMovementId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Backend-independent semantic lane movement before curve generation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CompiledMovement {
    from: CompiledLaneId,
    to: CompiledLaneId,
    junction_id: JunctionId,
}

impl CompiledMovement {
    #[must_use]
    pub const fn new(from: CompiledLaneId, to: CompiledLaneId, junction_id: JunctionId) -> Self {
        Self {
            from,
            to,
            junction_id,
        }
    }

    #[must_use]
    pub const fn from(self) -> CompiledLaneId {
        self.from
    }

    #[must_use]
    pub const fn to(self) -> CompiledLaneId {
        self.to
    }

    #[must_use]
    pub const fn junction_id(self) -> JunctionId {
        self.junction_id
    }
}

/// Deterministically indexed semantic movements. Curves/conflicts are E07-T06.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MovementTable {
    lane_count: u32,
    movements: Vec<CompiledMovement>,
}

impl MovementTable {
    pub fn new(
        lane_count: u32,
        mut movements: Vec<CompiledMovement>,
    ) -> Result<Self, MovementError> {
        if lane_count > MAX_GRAPH_NODES {
            return Err(MovementError(MovementErrorCode::TooManyLanes));
        }
        if movements.len() > MAX_GRAPH_EDGES {
            return Err(MovementError(MovementErrorCode::TooManyMovements));
        }
        movements.sort_unstable();
        if movements.iter().any(|movement| {
            movement.from().get() >= lane_count || movement.to().get() >= lane_count
        }) {
            return Err(MovementError(MovementErrorCode::LaneOutsideTable));
        }
        if movements
            .iter()
            .any(|movement| movement.from() == movement.to())
        {
            return Err(MovementError(MovementErrorCode::SelfMovement));
        }
        for pair in movements.windows(2) {
            if pair[0] == pair[1] {
                return Err(MovementError(MovementErrorCode::DuplicateMovement));
            }
            if pair[0].from() == pair[1].from() && pair[0].to() == pair[1].to() {
                return Err(MovementError(MovementErrorCode::LanePairAmbiguous));
            }
        }
        Ok(Self {
            lane_count,
            movements,
        })
    }

    #[must_use]
    pub const fn lane_count(&self) -> u32 {
        self.lane_count
    }

    #[must_use]
    pub fn movements(&self) -> &[CompiledMovement] {
        &self.movements
    }

    #[must_use]
    pub fn movement(&self, id: CompiledMovementId) -> Option<CompiledMovement> {
        self.movements.get(id.get() as usize).copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementErrorCode {
    TooManyLanes,
    TooManyMovements,
    LaneOutsideTable,
    SelfMovement,
    DuplicateMovement,
    LanePairAmbiguous,
}

impl MovementErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyLanes => "csn.movement.lanes.too_many",
            Self::TooManyMovements => "csn.movement.count.too_many",
            Self::LaneOutsideTable => "csn.movement.lane.outside_table",
            Self::SelfMovement => "csn.movement.self",
            Self::DuplicateMovement => "csn.movement.duplicate",
            Self::LanePairAmbiguous => "csn.movement.lane_pair.ambiguous",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementError(MovementErrorCode);

impl MovementError {
    #[must_use]
    pub const fn code(self) -> MovementErrorCode {
        self.0
    }
}

impl fmt::Display for MovementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for MovementError {}

/// Exact cubic control polygon for one junction movement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CompiledMovementCurve {
    movement_id: CompiledMovementId,
    start: CompiledPoint,
    control_1: CompiledPoint,
    control_2: CompiledPoint,
    end: CompiledPoint,
}

impl CompiledMovementCurve {
    #[must_use]
    pub const fn new(
        movement_id: CompiledMovementId,
        start: CompiledPoint,
        control_1: CompiledPoint,
        control_2: CompiledPoint,
        end: CompiledPoint,
    ) -> Self {
        Self {
            movement_id,
            start,
            control_1,
            control_2,
            end,
        }
    }

    #[must_use]
    pub const fn movement_id(self) -> CompiledMovementId {
        self.movement_id
    }

    #[must_use]
    pub const fn start(self) -> CompiledPoint {
        self.start
    }

    #[must_use]
    pub const fn control_1(self) -> CompiledPoint {
        self.control_1
    }

    #[must_use]
    pub const fn control_2(self) -> CompiledPoint {
        self.control_2
    }

    #[must_use]
    pub const fn end(self) -> CompiledPoint {
        self.end
    }
}

/// Geometric reason two movement centerlines produce one conflict zone.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledConflictKind {
    Crossing,
    Overlap,
}

/// Conservative local-metric AABB around one pair's geometric conflict sites.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CompiledConflictZone {
    first: CompiledMovementId,
    second: CompiledMovementId,
    kind: CompiledConflictKind,
    min: CompiledPoint,
    max: CompiledPoint,
}

impl CompiledConflictZone {
    pub fn new(
        first: CompiledMovementId,
        second: CompiledMovementId,
        kind: CompiledConflictKind,
        min: CompiledPoint,
        max: CompiledPoint,
    ) -> Result<Self, MovementGeometryError> {
        if first >= second {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::ConflictPairNotCanonical,
            ));
        }
        if !point_is_finite(min)
            || !point_is_finite(max)
            || min.x_m() >= max.x_m()
            || min.y_m() >= max.y_m()
        {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::ConflictBoundsInvalid,
            ));
        }
        Ok(Self {
            first,
            second,
            kind,
            min,
            max,
        })
    }

    #[must_use]
    pub const fn first(self) -> CompiledMovementId {
        self.first
    }

    #[must_use]
    pub const fn second(self) -> CompiledMovementId {
        self.second
    }

    #[must_use]
    pub const fn kind(self) -> CompiledConflictKind {
        self.kind
    }

    #[must_use]
    pub const fn min(self) -> CompiledPoint {
        self.min
    }

    #[must_use]
    pub const fn max(self) -> CompiledPoint {
        self.max
    }
}

/// Deterministic movement curves and sparse symmetric conflict matrix.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MovementGeometryTable {
    movement_count: u32,
    curves: Vec<CompiledMovementCurve>,
    conflict_zones: Vec<CompiledConflictZone>,
}

impl MovementGeometryTable {
    pub fn new(
        movement_count: u32,
        mut curves: Vec<CompiledMovementCurve>,
        mut conflict_zones: Vec<CompiledConflictZone>,
    ) -> Result<Self, MovementGeometryError> {
        if movement_count as usize > MAX_GRAPH_EDGES {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::TooManyMovements,
            ));
        }
        if curves.len() != movement_count as usize {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::CurveCountMismatch,
            ));
        }
        curves.sort_by_key(|curve| curve.movement_id());
        if curves.iter().enumerate().any(|(index, curve)| {
            curve.movement_id().get() as usize != index
                || !point_is_finite(curve.start())
                || !point_is_finite(curve.control_1())
                || !point_is_finite(curve.control_2())
                || !point_is_finite(curve.end())
        }) {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::CurveInvalid,
            ));
        }
        if conflict_zones.len() > MAX_GRAPH_EDGES {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::TooManyConflicts,
            ));
        }
        conflict_zones.sort_by_key(|zone| (zone.first(), zone.second()));
        for zone in &conflict_zones {
            if zone.second().get() >= movement_count {
                return Err(MovementGeometryError(
                    MovementGeometryErrorCode::ConflictMovementOutsideTable,
                ));
            }
        }
        if conflict_zones
            .windows(2)
            .any(|pair| pair[0].first() == pair[1].first() && pair[0].second() == pair[1].second())
        {
            return Err(MovementGeometryError(
                MovementGeometryErrorCode::DuplicateConflictPair,
            ));
        }
        Ok(Self {
            movement_count,
            curves,
            conflict_zones,
        })
    }

    #[must_use]
    pub const fn movement_count(&self) -> u32 {
        self.movement_count
    }

    #[must_use]
    pub fn curves(&self) -> &[CompiledMovementCurve] {
        &self.curves
    }

    #[must_use]
    pub fn curve(&self, id: CompiledMovementId) -> Option<CompiledMovementCurve> {
        self.curves.get(id.get() as usize).copied()
    }

    #[must_use]
    pub fn conflict_zones(&self) -> &[CompiledConflictZone] {
        &self.conflict_zones
    }

    #[must_use]
    pub fn conflicts(&self, first: CompiledMovementId, second: CompiledMovementId) -> bool {
        if first == second {
            return false;
        }
        let pair = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        self.conflict_zones
            .binary_search_by_key(&pair, |zone| (zone.first(), zone.second()))
            .is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementGeometryErrorCode {
    TooManyMovements,
    CurveCountMismatch,
    CurveInvalid,
    TooManyConflicts,
    ConflictPairNotCanonical,
    ConflictMovementOutsideTable,
    ConflictBoundsInvalid,
    DuplicateConflictPair,
}

impl MovementGeometryErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyMovements => "csn.movement_geometry.movements.too_many",
            Self::CurveCountMismatch => "csn.movement_geometry.curves.count_mismatch",
            Self::CurveInvalid => "csn.movement_geometry.curve.invalid",
            Self::TooManyConflicts => "csn.movement_geometry.conflicts.too_many",
            Self::ConflictPairNotCanonical => "csn.conflict.pair.not_canonical",
            Self::ConflictMovementOutsideTable => "csn.conflict.movement.outside_table",
            Self::ConflictBoundsInvalid => "csn.conflict.bounds.invalid",
            Self::DuplicateConflictPair => "csn.conflict.pair.duplicate",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementGeometryError(MovementGeometryErrorCode);

impl MovementGeometryError {
    #[must_use]
    pub const fn code(self) -> MovementGeometryErrorCode {
        self.0
    }
}

impl fmt::Display for MovementGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for MovementGeometryError {}

fn point_is_finite(point: CompiledPoint) -> bool {
    point.x_m().is_finite() && point.y_m().is_finite()
}

/// Compact pedestrian graph node ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CompiledPedestrianNodeId(u32);

impl CompiledPedestrianNodeId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Design origin retained for every pedestrian graph node.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum PedestrianNodeOrigin {
    WalkingArea(WalkingAreaId),
    Sidewalk(SidewalkId),
}

/// One directed pedestrian link sourced from a Design crossing.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PedestrianAdjacency {
    from: CompiledPedestrianNodeId,
    to: CompiledPedestrianNodeId,
    crossing_id: CrossingId,
}

impl PedestrianAdjacency {
    #[must_use]
    pub const fn new(
        from: CompiledPedestrianNodeId,
        to: CompiledPedestrianNodeId,
        crossing_id: CrossingId,
    ) -> Self {
        Self {
            from,
            to,
            crossing_id,
        }
    }

    #[must_use]
    pub const fn from(self) -> CompiledPedestrianNodeId {
        self.from
    }

    #[must_use]
    pub const fn to(self) -> CompiledPedestrianNodeId {
        self.to
    }

    #[must_use]
    pub const fn crossing_id(self) -> CrossingId {
        self.crossing_id
    }
}

/// Compact pedestrian reachability graph with complete node/link source maps.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PedestrianGraph {
    origins: Vec<PedestrianNodeOrigin>,
    adjacency: Vec<PedestrianAdjacency>,
}

impl PedestrianGraph {
    pub fn new(
        origins: Vec<PedestrianNodeOrigin>,
        mut adjacency: Vec<PedestrianAdjacency>,
    ) -> Result<Self, GraphError> {
        if origins.len() > MAX_GRAPH_NODES as usize {
            return Err(GraphError(GraphErrorCode::TooManyNodes));
        }
        if adjacency.len() > MAX_GRAPH_EDGES {
            return Err(GraphError(GraphErrorCode::TooManyEdges));
        }
        let mut unique_origins = origins.clone();
        unique_origins.sort_unstable();
        unique_origins.dedup();
        if unique_origins.len() != origins.len() {
            return Err(GraphError(GraphErrorCode::DuplicateOrigin));
        }
        adjacency.sort_unstable();
        adjacency.dedup();
        let node_count = origins.len() as u32;
        if adjacency
            .iter()
            .any(|edge| edge.from().get() >= node_count || edge.to().get() >= node_count)
        {
            return Err(GraphError(GraphErrorCode::NodeOutsideGraph));
        }
        Ok(Self { origins, adjacency })
    }

    #[must_use]
    pub fn origins(&self) -> &[PedestrianNodeOrigin] {
        &self.origins
    }

    #[must_use]
    pub fn adjacency(&self) -> &[PedestrianAdjacency] {
        &self.adjacency
    }

    #[must_use]
    pub fn node_for_walking_area(&self, id: WalkingAreaId) -> Option<CompiledPedestrianNodeId> {
        self.origins
            .iter()
            .position(|origin| *origin == PedestrianNodeOrigin::WalkingArea(id))
            .map(|index| CompiledPedestrianNodeId::new(index as u32))
    }

    #[must_use]
    pub fn can_reach(&self, from: CompiledPedestrianNodeId, to: CompiledPedestrianNodeId) -> bool {
        can_reach(
            self.origins.len() as u32,
            from.get(),
            to.get(),
            self.adjacency
                .iter()
                .map(|edge| (edge.from().get(), edge.to().get())),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphErrorCode {
    TooManyNodes,
    TooManyEdges,
    DuplicateOrigin,
    NodeOutsideGraph,
}

impl GraphErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyNodes => "csn.graph.nodes.too_many",
            Self::TooManyEdges => "csn.graph.edges.too_many",
            Self::DuplicateOrigin => "csn.graph.origin.duplicate",
            Self::NodeOutsideGraph => "csn.graph.node.outside_graph",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphError(GraphErrorCode);

impl GraphError {
    #[must_use]
    pub const fn code(self) -> GraphErrorCode {
        self.0
    }
}

impl fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for GraphError {}

fn can_reach(
    node_count: u32,
    from: u32,
    to: u32,
    edges: impl IntoIterator<Item = (u32, u32)>,
) -> bool {
    if from >= node_count || to >= node_count {
        return false;
    }
    if from == to {
        return true;
    }
    let mut edges: Vec<_> = edges.into_iter().collect();
    edges.sort_unstable();
    let mut visited = vec![false; node_count as usize];
    let mut pending = vec![from];
    visited[from as usize] = true;
    while let Some(node) = pending.pop() {
        let start = edges.partition_point(|(source, _)| *source < node);
        let end = edges.partition_point(|(source, _)| *source <= node);
        for (_, target) in &edges[start..end] {
            if *target == to {
                return true;
            }
            if !visited[*target as usize] {
                visited[*target as usize] = true;
                pending.push(*target);
            }
        }
    }
    false
}

/// Backend-independent semantic use retained for capability negotiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledLaneUse {
    GeneralTraffic,
    BusOnly,
    Bicycle,
    Parking,
}

/// One point in the local metric CSN coordinate frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CompiledPoint {
    x_m: f64,
    y_m: f64,
}

impl CompiledPoint {
    #[must_use]
    pub const fn new(x_m: f64, y_m: f64) -> Self {
        Self { x_m, y_m }
    }

    #[must_use]
    pub const fn x_m(self) -> f64 {
        self.x_m
    }

    #[must_use]
    pub const fn y_m(self) -> f64 {
        self.y_m
    }
}

/// Origin mapping for one compact compiled lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LaneOrigin {
    corridor_id: CorridorId,
    lane_id: LaneId,
}

impl LaneOrigin {
    #[must_use]
    pub const fn new(corridor_id: CorridorId, lane_id: LaneId) -> Self {
        Self {
            corridor_id,
            lane_id,
        }
    }

    #[must_use]
    pub const fn corridor_id(self) -> CorridorId {
        self.corridor_id
    }

    #[must_use]
    pub const fn lane_id(self) -> LaneId {
        self.lane_id
    }
}

/// Parallel hot arrays for the first compiled road-lane contract.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaneTable {
    starts: Vec<CompiledPoint>,
    ends: Vec<CompiledPoint>,
    widths_m: Vec<f64>,
    uses: Vec<CompiledLaneUse>,
}

impl LaneTable {
    #[must_use]
    pub fn new(
        starts: Vec<CompiledPoint>,
        ends: Vec<CompiledPoint>,
        widths_m: Vec<f64>,
        uses: Vec<CompiledLaneUse>,
    ) -> Option<Self> {
        let len = starts.len();
        if ends.len() != len
            || widths_m.len() != len
            || uses.len() != len
            || len > u32::MAX as usize
            || starts
                .iter()
                .chain(&ends)
                .any(|point| !point.x_m().is_finite() || !point.y_m().is_finite())
            || starts.iter().zip(&ends).any(|(start, end)| start == end)
            || widths_m
                .iter()
                .any(|width| !width.is_finite() || *width <= 0.0)
        {
            return None;
        }
        Some(Self {
            starts,
            ends,
            widths_m,
            uses,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    #[must_use]
    pub fn lane(&self, id: CompiledLaneId) -> Option<CompiledLaneRef> {
        let index = id.get() as usize;
        Some(CompiledLaneRef {
            id,
            start: *self.starts.get(index)?,
            end: *self.ends.get(index)?,
            width_m: *self.widths_m.get(index)?,
            use_kind: *self.uses.get(index)?,
        })
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CompiledLaneRef> + '_ {
        (0..self.len()).map(|index| {
            self.lane(CompiledLaneId::new(index as u32))
                .expect("LaneTable constructor guarantees aligned arrays")
        })
    }
}

/// Read-only view over one row of [`LaneTable`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledLaneRef {
    id: CompiledLaneId,
    start: CompiledPoint,
    end: CompiledPoint,
    width_m: f64,
    use_kind: CompiledLaneUse,
}

impl CompiledLaneRef {
    #[must_use]
    pub const fn id(self) -> CompiledLaneId {
        self.id
    }

    #[must_use]
    pub const fn start(self) -> CompiledPoint {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> CompiledPoint {
        self.end
    }

    #[must_use]
    pub const fn width_m(self) -> f64 {
        self.width_m
    }

    #[must_use]
    pub const fn use_kind(self) -> CompiledLaneUse {
        self.use_kind
    }

    #[must_use]
    pub fn length_m(self) -> f64 {
        (self.end.y_m() - self.start.y_m()).hypot(self.end.x_m() - self.start.x_m())
    }
}

/// Header binds a complete CSN to its exact source observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CompiledNetworkHeader {
    schema_version: u32,
    source_revision: SourceRevision,
    coordinate_frame: CoordinateFrame,
    content_hash: Sha256Digest,
}

impl CompiledNetworkHeader {
    #[must_use]
    pub const fn new(source_revision: SourceRevision, content_hash: Sha256Digest) -> Self {
        Self {
            schema_version: COMPILED_NETWORK_SCHEMA_VERSION,
            source_revision,
            coordinate_frame: CoordinateFrame::LocalMetric,
            content_hash,
        }
    }

    #[must_use]
    pub const fn schema_version(self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn source_revision(self) -> SourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn coordinate_frame(self) -> CoordinateFrame {
        self.coordinate_frame
    }

    #[must_use]
    pub const fn content_hash(self) -> Sha256Digest {
        self.content_hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkErrorCode {
    SourceMapLengthMismatch,
    LaneGraphLengthMismatch,
    MovementTableLengthMismatch,
    MovementGraphMismatch,
    MovementGeometryCountMismatch,
    GraphLimitExceeded,
}

impl NetworkErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceMapLengthMismatch => "csn.source_map.length_mismatch",
            Self::LaneGraphLengthMismatch => "csn.lane_graph.length_mismatch",
            Self::MovementTableLengthMismatch => "csn.movement_table.length_mismatch",
            Self::MovementGraphMismatch => "csn.movement_graph.mismatch",
            Self::MovementGeometryCountMismatch => "csn.movement_geometry.count_mismatch",
            Self::GraphLimitExceeded => "csn.graph.limit_exceeded",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkError(NetworkErrorCode);

impl NetworkError {
    #[must_use]
    pub const fn code(self) -> NetworkErrorCode {
        self.0
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for NetworkError {}

/// Backend-independent topology grouped as one constructor boundary.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompiledTopology {
    lane_graph: LaneGraph,
    movements: MovementTable,
    movement_geometry: MovementGeometryTable,
    pedestrian_graph: PedestrianGraph,
}

impl CompiledTopology {
    #[must_use]
    pub const fn new(
        lane_graph: LaneGraph,
        movements: MovementTable,
        movement_geometry: MovementGeometryTable,
        pedestrian_graph: PedestrianGraph,
    ) -> Self {
        Self {
            lane_graph,
            movements,
            movement_geometry,
            pedestrian_graph,
        }
    }

    #[must_use]
    pub const fn lane_graph(&self) -> &LaneGraph {
        &self.lane_graph
    }

    #[must_use]
    pub const fn movements(&self) -> &MovementTable {
        &self.movements
    }

    #[must_use]
    pub const fn movement_geometry(&self) -> &MovementGeometryTable {
        &self.movement_geometry
    }

    #[must_use]
    pub const fn pedestrian_graph(&self) -> &PedestrianGraph {
        &self.pedestrian_graph
    }
}

/// Complete immutable network published atomically after successful compilation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompiledNetwork {
    header: CompiledNetworkHeader,
    lanes: LaneTable,
    lane_origins: Vec<LaneOrigin>,
    topology: CompiledTopology,
    requirements: CapabilityRequirements,
}

impl CompiledNetwork {
    pub fn new(
        header: CompiledNetworkHeader,
        lanes: LaneTable,
        lane_origins: Vec<LaneOrigin>,
        requirements: CapabilityRequirements,
    ) -> Result<Self, NetworkError> {
        let lane_graph = LaneGraph::new(lanes.len() as u32, Vec::new())
            .map_err(|_| NetworkError(NetworkErrorCode::GraphLimitExceeded))?;
        let movements = MovementTable::new(lanes.len() as u32, Vec::new())
            .map_err(|_| NetworkError(NetworkErrorCode::GraphLimitExceeded))?;
        let movement_geometry = MovementGeometryTable::new(0, Vec::new(), Vec::new())
            .map_err(|_| NetworkError(NetworkErrorCode::GraphLimitExceeded))?;
        let pedestrian_graph =
            PedestrianGraph::new(Vec::new(), Vec::new()).expect("empty graph is valid");
        Self::new_with_graphs(
            header,
            lanes,
            lane_origins,
            CompiledTopology::new(lane_graph, movements, movement_geometry, pedestrian_graph),
            requirements,
        )
    }

    pub fn new_with_graphs(
        header: CompiledNetworkHeader,
        lanes: LaneTable,
        lane_origins: Vec<LaneOrigin>,
        topology: CompiledTopology,
        requirements: CapabilityRequirements,
    ) -> Result<Self, NetworkError> {
        if lanes.len() != lane_origins.len() {
            return Err(NetworkError(NetworkErrorCode::SourceMapLengthMismatch));
        }
        if lanes.len() != topology.lane_graph().node_count() as usize {
            return Err(NetworkError(NetworkErrorCode::LaneGraphLengthMismatch));
        }
        if lanes.len() != topology.movements().lane_count() as usize {
            return Err(NetworkError(NetworkErrorCode::MovementTableLengthMismatch));
        }
        if topology.movements().movements().len()
            != topology.movement_geometry().movement_count() as usize
        {
            return Err(NetworkError(
                NetworkErrorCode::MovementGeometryCountMismatch,
            ));
        }
        if topology.movements().movements().iter().any(|movement| {
            topology
                .lane_graph()
                .adjacency()
                .binary_search(&LaneAdjacency::new(
                    movement.from(),
                    movement.to(),
                    movement.junction_id(),
                ))
                .is_err()
        }) {
            return Err(NetworkError(NetworkErrorCode::MovementGraphMismatch));
        }
        Ok(Self {
            header,
            lanes,
            lane_origins,
            topology,
            requirements,
        })
    }

    #[must_use]
    pub const fn header(&self) -> CompiledNetworkHeader {
        self.header
    }

    #[must_use]
    pub const fn lanes(&self) -> &LaneTable {
        &self.lanes
    }

    #[must_use]
    pub fn lane_origin(&self, id: CompiledLaneId) -> Option<LaneOrigin> {
        self.lane_origins.get(id.get() as usize).copied()
    }

    #[must_use]
    pub const fn lane_graph(&self) -> &LaneGraph {
        self.topology.lane_graph()
    }

    #[must_use]
    pub const fn movements(&self) -> &MovementTable {
        self.topology.movements()
    }

    #[must_use]
    pub const fn movement_geometry(&self) -> &MovementGeometryTable {
        self.topology.movement_geometry()
    }

    #[must_use]
    pub const fn pedestrian_graph(&self) -> &PedestrianGraph {
        self.topology.pedestrian_graph()
    }

    #[must_use]
    pub const fn requirements(&self) -> &CapabilityRequirements {
        &self.requirements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_table_rejects_misaligned_or_invalid_arrays() {
        let point = CompiledPoint::new(0.0, 0.0);
        assert!(LaneTable::new(vec![point], vec![], vec![3.5], vec![]).is_none());
        assert!(
            LaneTable::new(
                vec![point],
                vec![CompiledPoint::new(f64::NAN, 0.0)],
                vec![3.5],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
        assert!(
            LaneTable::new(
                vec![point],
                vec![point],
                vec![3.5],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
        assert!(
            LaneTable::new(
                vec![point],
                vec![CompiledPoint::new(1.0, 0.0)],
                vec![0.0],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
    }

    #[test]
    fn network_requires_one_origin_per_compact_lane() {
        let lanes = LaneTable::new(vec![], vec![], vec![], vec![]).unwrap();
        let header =
            CompiledNetworkHeader::new(SourceRevision::new(0), Sha256Digest::from_bytes([0; 32]));
        let error = CompiledNetwork::new(
            header,
            lanes,
            vec![LaneOrigin::new(
                CorridorId::from_u128(1),
                LaneId::from_u128(2),
            )],
            CapabilityRequirements::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), NetworkErrorCode::SourceMapLengthMismatch);
    }

    #[test]
    fn capability_manifest_uses_stable_public_ids() {
        let requirements = CapabilityRequirements::new([
            CapabilityId::TransitBusLanes,
            CapabilityId::RoadVehiclesBasic,
        ]);
        assert_eq!(
            serde_json::to_string(&requirements).unwrap(),
            r#"["road.vehicles.basic","transit.bus_lanes"]"#
        );
    }

    #[test]
    fn compact_graphs_validate_edges_and_answer_directed_reachability() {
        let graph = LaneGraph::new(
            3,
            vec![
                LaneAdjacency::new(
                    CompiledLaneId::new(1),
                    CompiledLaneId::new(2),
                    JunctionId::from_u128(2),
                ),
                LaneAdjacency::new(
                    CompiledLaneId::new(0),
                    CompiledLaneId::new(1),
                    JunctionId::from_u128(1),
                ),
            ],
        )
        .unwrap();
        assert!(graph.can_reach(CompiledLaneId::new(0), CompiledLaneId::new(2)));
        assert!(!graph.can_reach(CompiledLaneId::new(2), CompiledLaneId::new(0)));
        assert_eq!(
            LaneGraph::new(
                1,
                vec![LaneAdjacency::new(
                    CompiledLaneId::new(0),
                    CompiledLaneId::new(1),
                    JunctionId::from_u128(1),
                )],
            )
            .unwrap_err()
            .code(),
            GraphErrorCode::NodeOutsideGraph
        );
        assert_eq!(
            LaneGraph::new(MAX_GRAPH_NODES + 1, Vec::new())
                .unwrap_err()
                .code(),
            GraphErrorCode::TooManyNodes
        );
    }

    #[test]
    fn movement_table_assigns_stable_compact_ids_and_rejects_ambiguity() {
        let first = CompiledMovement::new(
            CompiledLaneId::new(0),
            CompiledLaneId::new(2),
            JunctionId::from_u128(2),
        );
        let second = CompiledMovement::new(
            CompiledLaneId::new(0),
            CompiledLaneId::new(1),
            JunctionId::from_u128(1),
        );
        let movements = MovementTable::new(3, vec![first, second]).unwrap();
        assert_eq!(movements.movement(CompiledMovementId::new(0)), Some(second));
        assert_eq!(movements.movement(CompiledMovementId::new(1)), Some(first));

        let ambiguous = MovementTable::new(
            2,
            vec![
                CompiledMovement::new(
                    CompiledLaneId::new(0),
                    CompiledLaneId::new(1),
                    JunctionId::from_u128(1),
                ),
                CompiledMovement::new(
                    CompiledLaneId::new(0),
                    CompiledLaneId::new(1),
                    JunctionId::from_u128(2),
                ),
            ],
        )
        .unwrap_err();
        assert_eq!(ambiguous.code(), MovementErrorCode::LanePairAmbiguous);
    }

    #[test]
    fn movement_geometry_assigns_curves_and_symmetric_conflict_matrix() {
        let curves = vec![
            CompiledMovementCurve::new(
                CompiledMovementId::new(1),
                CompiledPoint::new(0.0, -1.0),
                CompiledPoint::new(1.0, -1.0),
                CompiledPoint::new(1.0, 0.0),
                CompiledPoint::new(1.0, 1.0),
            ),
            CompiledMovementCurve::new(
                CompiledMovementId::new(0),
                CompiledPoint::new(-1.0, 0.0),
                CompiledPoint::new(0.0, 0.0),
                CompiledPoint::new(0.0, 0.0),
                CompiledPoint::new(1.0, 0.0),
            ),
        ];
        let zone = CompiledConflictZone::new(
            CompiledMovementId::new(0),
            CompiledMovementId::new(1),
            CompiledConflictKind::Crossing,
            CompiledPoint::new(-1.0, -1.0),
            CompiledPoint::new(1.0, 1.0),
        )
        .unwrap();
        let geometry = MovementGeometryTable::new(2, curves, vec![zone]).unwrap();

        assert_eq!(
            geometry.curve(CompiledMovementId::new(0)).unwrap().start(),
            CompiledPoint::new(-1.0, 0.0)
        );
        assert!(geometry.conflicts(CompiledMovementId::new(0), CompiledMovementId::new(1)));
        assert!(geometry.conflicts(CompiledMovementId::new(1), CompiledMovementId::new(0)));
        assert!(!geometry.conflicts(CompiledMovementId::new(0), CompiledMovementId::new(0)));

        let invalid = MovementGeometryTable::new(
            1,
            vec![CompiledMovementCurve::new(
                CompiledMovementId::new(1),
                CompiledPoint::new(0.0, 0.0),
                CompiledPoint::new(1.0, 0.0),
                CompiledPoint::new(2.0, 0.0),
                CompiledPoint::new(3.0, 0.0),
            )],
            Vec::new(),
        )
        .unwrap_err();
        assert_eq!(invalid.code(), MovementGeometryErrorCode::CurveInvalid);
    }

    #[test]
    fn network_requires_every_movement_to_have_a_coarse_graph_edge() {
        let lanes = LaneTable::new(
            vec![CompiledPoint::new(0.0, 0.0), CompiledPoint::new(1.0, 0.0)],
            vec![CompiledPoint::new(1.0, 0.0), CompiledPoint::new(2.0, 0.0)],
            vec![3.5, 3.5],
            vec![
                CompiledLaneUse::GeneralTraffic,
                CompiledLaneUse::GeneralTraffic,
            ],
        )
        .unwrap();
        let junction = JunctionId::from_u128(3);
        let lane_graph = LaneGraph::new(
            2,
            vec![LaneAdjacency::new(
                CompiledLaneId::new(0),
                CompiledLaneId::new(1),
                junction,
            )],
        )
        .unwrap();
        let movements = MovementTable::new(
            2,
            vec![CompiledMovement::new(
                CompiledLaneId::new(1),
                CompiledLaneId::new(0),
                junction,
            )],
        )
        .unwrap();
        let error = CompiledNetwork::new_with_graphs(
            CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([1; 32])),
            lanes,
            vec![
                LaneOrigin::new(CorridorId::from_u128(10), LaneId::from_u128(20)),
                LaneOrigin::new(CorridorId::from_u128(11), LaneId::from_u128(21)),
            ],
            CompiledTopology::new(
                lane_graph,
                movements,
                MovementGeometryTable::new(
                    1,
                    vec![CompiledMovementCurve::new(
                        CompiledMovementId::new(0),
                        CompiledPoint::new(0.0, 0.0),
                        CompiledPoint::new(0.25, 0.0),
                        CompiledPoint::new(0.75, 0.0),
                        CompiledPoint::new(1.0, 0.0),
                    )],
                    Vec::new(),
                )
                .unwrap(),
                PedestrianGraph::new(Vec::new(), Vec::new()).unwrap(),
            ),
            CapabilityRequirements::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), NetworkErrorCode::MovementGraphMismatch);
    }

    #[test]
    fn coarse_lane_graph_may_retain_candidates_not_promoted_to_movements() {
        let lanes = LaneTable::new(
            vec![CompiledPoint::new(0.0, 0.0), CompiledPoint::new(1.0, 0.0)],
            vec![CompiledPoint::new(1.0, 0.0), CompiledPoint::new(2.0, 0.0)],
            vec![3.5, 3.5],
            vec![
                CompiledLaneUse::GeneralTraffic,
                CompiledLaneUse::GeneralTraffic,
            ],
        )
        .unwrap();
        let lane_graph = LaneGraph::new(
            2,
            vec![LaneAdjacency::new(
                CompiledLaneId::new(0),
                CompiledLaneId::new(1),
                JunctionId::from_u128(3),
            )],
        )
        .unwrap();
        let network = CompiledNetwork::new_with_graphs(
            CompiledNetworkHeader::new(SourceRevision::new(1), Sha256Digest::from_bytes([1; 32])),
            lanes,
            vec![
                LaneOrigin::new(CorridorId::from_u128(10), LaneId::from_u128(20)),
                LaneOrigin::new(CorridorId::from_u128(11), LaneId::from_u128(21)),
            ],
            CompiledTopology::new(
                lane_graph,
                MovementTable::new(2, Vec::new()).unwrap(),
                MovementGeometryTable::new(0, Vec::new(), Vec::new()).unwrap(),
                PedestrianGraph::new(Vec::new(), Vec::new()).unwrap(),
            ),
            CapabilityRequirements::default(),
        )
        .unwrap();

        assert!(network.movements().movements().is_empty());
        assert_eq!(network.lane_graph().adjacency().len(), 1);
    }

    #[test]
    fn pedestrian_graph_retains_node_and_crossing_sources() {
        let first = WalkingAreaId::from_u128(1);
        let second = WalkingAreaId::from_u128(2);
        let crossing = CrossingId::from_u128(3);
        let graph = PedestrianGraph::new(
            vec![
                PedestrianNodeOrigin::WalkingArea(first),
                PedestrianNodeOrigin::WalkingArea(second),
            ],
            vec![
                PedestrianAdjacency::new(
                    CompiledPedestrianNodeId::new(0),
                    CompiledPedestrianNodeId::new(1),
                    crossing,
                ),
                PedestrianAdjacency::new(
                    CompiledPedestrianNodeId::new(1),
                    CompiledPedestrianNodeId::new(0),
                    crossing,
                ),
            ],
        )
        .unwrap();
        let first_node = graph.node_for_walking_area(first).unwrap();
        let second_node = graph.node_for_walking_area(second).unwrap();
        assert!(graph.can_reach(first_node, second_node));
        assert_eq!(graph.adjacency()[0].crossing_id(), crossing);
    }
}
