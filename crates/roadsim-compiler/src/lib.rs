//! Deterministic Design Model to Compiled Simulation Network pipeline.
//!
//! The current vertical slice accepts straight corridors with one constant cross
//! section and compiles exact cubic junction movement connectors. Unsupported
//! geometry is rejected with a stable diagnostic instead of being simplified
//! silently.

use roadsim_compiled_network::{
    COMPILED_NETWORK_SCHEMA_VERSION, CapabilityId, CapabilityRequirements, CompiledConflictKind,
    CompiledConflictZone, CompiledLaneId, CompiledLaneUse, CompiledMovement, CompiledMovementCurve,
    CompiledMovementId, CompiledNetwork, CompiledNetworkHeader, CompiledPedestrianNodeId,
    CompiledPoint, CompiledTopology, LaneAdjacency, LaneGraph, LaneOrigin, LaneTable,
    MAX_GRAPH_EDGES, MovementGeometryTable, MovementTable, PedestrianAdjacency, PedestrianGraph,
    PedestrianNodeOrigin, SourceRevision,
};
use roadsim_domain::{
    Corridor, CorridorEnd, DemandEndpoint, DemandMode, LaneDirection, LaneSlice, LaneUse,
    Point2Meters, Project, ReferenceLineElementKind,
};
use roadsim_geometry::{
    CubicBezier2, Segment2, SegmentIntersection, segment_intersection, tessellate_cubic,
};
pub use roadsim_geometry::{GeometryContext, TessellationOptions};
use roadsim_types::{CoordinateMeters, ObjectRef, Sha256Digest};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const MAX_CONFLICT_SEGMENT_TESTS: u64 = 10_000_000;
pub const MAX_TOTAL_MOVEMENT_POINTS: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileOptionsErrorCode {
    InvalidApproachCutback,
    InvalidMovementPointLimit,
    InvalidConflictTestLimit,
}

impl CompileOptionsErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidApproachCutback => "compiler.options.approach_cutback.invalid",
            Self::InvalidMovementPointLimit => "compiler.options.movement_points.invalid",
            Self::InvalidConflictTestLimit => "compiler.options.conflict_tests.invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileOptionsError(CompileOptionsErrorCode);

impl CompileOptionsError {
    #[must_use]
    pub const fn code(self) -> CompileOptionsErrorCode {
        self.0
    }
}

impl fmt::Display for CompileOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for CompileOptionsError {}

/// Explicit numerical and resource policy for deterministic compilation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompileOptions {
    geometry_context: GeometryContext,
    movement_tessellation: TessellationOptions,
    approach_cutback_m: f64,
    max_total_movement_points: u64,
    max_conflict_segment_tests: u64,
}

impl CompileOptions {
    pub fn new(
        geometry_context: GeometryContext,
        movement_tessellation: TessellationOptions,
        approach_cutback_m: f64,
        max_total_movement_points: u64,
        max_conflict_segment_tests: u64,
    ) -> Result<Self, CompileOptionsError> {
        if !approach_cutback_m.is_finite() || approach_cutback_m <= 0.0 {
            return Err(CompileOptionsError(
                CompileOptionsErrorCode::InvalidApproachCutback,
            ));
        }
        if !(2..=MAX_TOTAL_MOVEMENT_POINTS).contains(&max_total_movement_points) {
            return Err(CompileOptionsError(
                CompileOptionsErrorCode::InvalidMovementPointLimit,
            ));
        }
        if max_conflict_segment_tests == 0
            || max_conflict_segment_tests > MAX_CONFLICT_SEGMENT_TESTS
        {
            return Err(CompileOptionsError(
                CompileOptionsErrorCode::InvalidConflictTestLimit,
            ));
        }
        Ok(Self {
            geometry_context,
            movement_tessellation,
            approach_cutback_m,
            max_total_movement_points,
            max_conflict_segment_tests,
        })
    }

    #[must_use]
    pub const fn geometry_context(self) -> GeometryContext {
        self.geometry_context
    }

    #[must_use]
    pub const fn movement_tessellation(self) -> TessellationOptions {
        self.movement_tessellation
    }

    #[must_use]
    pub const fn approach_cutback_m(self) -> f64 {
        self.approach_cutback_m
    }

    #[must_use]
    pub const fn max_total_movement_points(self) -> u64 {
        self.max_total_movement_points
    }

    #[must_use]
    pub const fn max_conflict_segment_tests(self) -> u64 {
        self.max_conflict_segment_tests
    }
}

/// Stable compilation failure classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileErrorCode {
    EmptyNetwork,
    UnsupportedReferenceElement,
    VariableCrossSectionUnsupported,
    LaneTableInvariant,
    LaneGraphInvariant,
    MovementAmbiguous,
    MovementInvariant,
    MovementGeometryDegenerate,
    MovementGeometryInvariant,
    MovementGeometryLimitExceeded,
    MovementConflictLimitExceeded,
    PedestrianGraphInvariant,
    DemandEndpointUnreachable,
    DemandModeUnsupported,
    NetworkInvariant,
}

impl CompileErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyNetwork => "compiler.network.empty",
            Self::UnsupportedReferenceElement => "compiler.reference_line.element_unsupported",
            Self::VariableCrossSectionUnsupported => {
                "compiler.corridor.variable_cross_section_unsupported"
            }
            Self::LaneTableInvariant => "compiler.lane_table.invariant",
            Self::LaneGraphInvariant => "compiler.lane_graph.invariant",
            Self::MovementAmbiguous => "compiler.movement.ambiguous",
            Self::MovementInvariant => "compiler.movement.invariant",
            Self::MovementGeometryDegenerate => "compiler.movement_geometry.degenerate",
            Self::MovementGeometryInvariant => "compiler.movement_geometry.invariant",
            Self::MovementGeometryLimitExceeded => "compiler.movement_geometry.limit_exceeded",
            Self::MovementConflictLimitExceeded => "compiler.movement_conflict.limit_exceeded",
            Self::PedestrianGraphInvariant => "compiler.pedestrian_graph.invariant",
            Self::DemandEndpointUnreachable => "compiler.demand.endpoint_unreachable",
            Self::DemandModeUnsupported => "compiler.demand.mode_unsupported",
            Self::NetworkInvariant => "compiler.network.invariant",
        }
    }
}

/// Compilation diagnostic with stable Design Model object references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    code: CompileErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl CompileError {
    fn new(code: CompileErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> CompileErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for CompileError {}

#[derive(Default)]
struct LaneBuilder {
    starts: Vec<CompiledPoint>,
    ends: Vec<CompiledPoint>,
    widths_m: Vec<f64>,
    uses: Vec<CompiledLaneUse>,
    origins: Vec<LaneOrigin>,
    requirements: BTreeSet<CapabilityId>,
}

/// Compiles a complete immutable CSN or returns without publishing partial state.
pub fn compile_project(
    project: &Project,
    source_revision: SourceRevision,
    options: CompileOptions,
) -> Result<CompiledNetwork, CompileError> {
    let mut builder = LaneBuilder::default();
    for corridor in project.design_catalog().corridors() {
        compile_corridor(corridor, &mut builder)?;
    }
    if builder.starts.is_empty() {
        return Err(CompileError::new(
            CompileErrorCode::EmptyNetwork,
            vec![project.id().into()],
        ));
    }

    let lanes = LaneTable::new(builder.starts, builder.ends, builder.widths_m, builder.uses)
        .ok_or_else(|| CompileError::new(CompileErrorCode::LaneTableInvariant, Vec::new()))?;
    let lane_graph = compile_lane_graph(project, &builder.origins, lanes.len())?;
    let movements = compile_movements(&builder.origins, &lane_graph)?;
    let movement_geometry =
        compile_movement_geometry(&lanes, &builder.origins, &movements, options)?;
    let pedestrian_graph = compile_pedestrian_graph(project)?;
    if !pedestrian_graph.origins().is_empty() {
        builder
            .requirements
            .insert(CapabilityId::PedestrianWalkingAreasBasic);
    }
    validate_demand_reachability(project, &builder.origins, &lane_graph, &pedestrian_graph)?;
    let requirements = CapabilityRequirements::new(builder.requirements);
    let topology =
        CompiledTopology::new(lane_graph, movements, movement_geometry, pedestrian_graph);
    let hash = content_hash(project, &lanes, &builder.origins, &topology, &requirements);
    CompiledNetwork::new_with_graphs(
        CompiledNetworkHeader::new(source_revision, hash),
        lanes,
        builder.origins,
        topology,
        requirements,
    )
    .map_err(|_| CompileError::new(CompileErrorCode::NetworkInvariant, Vec::new()))
}

fn compile_corridor(corridor: &Corridor, output: &mut LaneBuilder) -> Result<(), CompileError> {
    if corridor.cross_section_profile().sections().len() != 1 {
        return Err(CompileError::new(
            CompileErrorCode::VariableCrossSectionUnsupported,
            vec![corridor.id().into()],
        ));
    }
    if corridor
        .reference_line()
        .segments()
        .iter()
        .any(|segment| segment.kind() != ReferenceLineElementKind::Line)
    {
        return Err(CompileError::new(
            CompileErrorCode::UnsupportedReferenceElement,
            vec![corridor.id().into()],
        ));
    }

    let start_pose = corridor.reference_line().start();
    let heading = start_pose.heading().get();
    let length = corridor.reference_line().total_length().get();
    let reference_start = start_pose.position();
    // `libm` keeps reference-line evaluation independent of platform libm.
    let tangent = (libm::cos(heading), libm::sin(heading));
    let left_normal = (-tangent.1, tangent.0);
    let reference_end = (
        reference_start.x_m().get() + tangent.0 * length,
        reference_start.y_m().get() + tangent.1 * length,
    );
    let layout = corridor.cross_section_profile().sections()[0].layout();
    compile_side(
        corridor,
        layout.left(),
        1.0,
        reference_end,
        left_normal,
        output,
    );
    compile_side(
        corridor,
        layout.right(),
        -1.0,
        reference_end,
        left_normal,
        output,
    );
    Ok(())
}

fn compile_side(
    corridor: &Corridor,
    slices: &[LaneSlice],
    side_sign: f64,
    reference_end: (f64, f64),
    left_normal: (f64, f64),
    output: &mut LaneBuilder,
) {
    let reference_start = corridor.reference_line().start().position();
    let mut outward_width = 0.0;
    for slice in slices {
        let definition = corridor
            .lane(slice.lane_id())
            .expect("Corridor constructor validates lane references");
        let center_offset = side_sign * (outward_width + slice.width().get() * 0.5);
        outward_width += slice.width().get();
        let start = CompiledPoint::new(
            reference_start.x_m().get() + left_normal.0 * center_offset,
            reference_start.y_m().get() + left_normal.1 * center_offset,
        );
        let end = CompiledPoint::new(
            reference_end.0 + left_normal.0 * center_offset,
            reference_end.1 + left_normal.1 * center_offset,
        );
        let (start, end) = match definition.direction() {
            LaneDirection::AlongReference => (start, end),
            LaneDirection::AgainstReference => (end, start),
        };
        let use_kind = compiled_use(definition.use_kind());
        output.starts.push(start);
        output.ends.push(end);
        output.widths_m.push(slice.width().get());
        output.uses.push(use_kind);
        output
            .origins
            .push(LaneOrigin::new(corridor.id(), definition.id()));
        output.requirements.insert(capability_for(use_kind));
    }
}

const fn compiled_use(use_kind: LaneUse) -> CompiledLaneUse {
    match use_kind {
        LaneUse::GeneralTraffic => CompiledLaneUse::GeneralTraffic,
        LaneUse::BusOnly => CompiledLaneUse::BusOnly,
        LaneUse::Bicycle => CompiledLaneUse::Bicycle,
        LaneUse::Parking => CompiledLaneUse::Parking,
    }
}

const fn capability_for(use_kind: CompiledLaneUse) -> CapabilityId {
    match use_kind {
        CompiledLaneUse::GeneralTraffic => CapabilityId::RoadVehiclesBasic,
        CompiledLaneUse::BusOnly => CapabilityId::TransitBusLanes,
        CompiledLaneUse::Bicycle => CapabilityId::BicycleLanes,
        CompiledLaneUse::Parking => CapabilityId::ParkingLanes,
    }
}

fn compile_lane_graph(
    project: &Project,
    origins: &[LaneOrigin],
    lane_count: usize,
) -> Result<LaneGraph, CompileError> {
    let lane_ids: BTreeMap<_, _> = origins
        .iter()
        .enumerate()
        .map(|(index, origin)| (origin.lane_id(), CompiledLaneId::new(index as u32)))
        .collect();
    let mut adjacency = Vec::new();
    for junction in project.design_catalog().junctions() {
        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        for approach in junction.approaches() {
            let corridor = project
                .design_catalog()
                .corridor(approach.corridor_id())
                .expect("DesignCatalog validates junction corridor references");
            for lane in corridor.lane_definitions() {
                let compiled_id = *lane_ids
                    .get(&lane.id())
                    .expect("corridor compilation publishes every lane origin");
                if lane_exits_at(lane.direction(), approach.end()) {
                    incoming.push((compiled_id, corridor.id()));
                }
                if lane_enters_at(lane.direction(), approach.end()) {
                    outgoing.push((compiled_id, corridor.id()));
                }
            }
        }
        for (from, from_corridor) in &incoming {
            for (to, to_corridor) in &outgoing {
                if from_corridor != to_corridor {
                    if adjacency.len() == MAX_GRAPH_EDGES {
                        return Err(CompileError::new(
                            CompileErrorCode::LaneGraphInvariant,
                            vec![junction.id().into()],
                        ));
                    }
                    adjacency.push(LaneAdjacency::new(*from, *to, junction.id()));
                }
            }
        }
    }
    LaneGraph::new(lane_count as u32, adjacency)
        .map_err(|_| CompileError::new(CompileErrorCode::LaneGraphInvariant, Vec::new()))
}

const fn lane_enters_at(direction: LaneDirection, end: CorridorEnd) -> bool {
    matches!(
        (direction, end),
        (LaneDirection::AlongReference, CorridorEnd::Start)
            | (LaneDirection::AgainstReference, CorridorEnd::End)
    )
}

const fn lane_exits_at(direction: LaneDirection, end: CorridorEnd) -> bool {
    matches!(
        (direction, end),
        (LaneDirection::AlongReference, CorridorEnd::End)
            | (LaneDirection::AgainstReference, CorridorEnd::Start)
    )
}

fn compile_movements(
    origins: &[LaneOrigin],
    lane_graph: &LaneGraph,
) -> Result<MovementTable, CompileError> {
    let mut targets = BTreeMap::<_, Vec<CompiledLaneId>>::new();
    for edge in lane_graph.adjacency() {
        let destination = origins[edge.to().get() as usize];
        targets
            .entry((edge.junction_id(), edge.from(), destination.corridor_id()))
            .or_default()
            .push(edge.to());
    }
    for ((junction_id, from, _), target_lanes) in targets {
        if target_lanes.len() > 1 {
            let source = origins[from.get() as usize];
            let mut object_refs = vec![
                junction_id.into(),
                source.corridor_id().into(),
                source.lane_id().into(),
            ];
            for target in target_lanes {
                let destination = origins[target.get() as usize];
                object_refs.push(destination.corridor_id().into());
                object_refs.push(destination.lane_id().into());
            }
            return Err(CompileError::new(
                CompileErrorCode::MovementAmbiguous,
                object_refs,
            ));
        }
    }
    let movements = lane_graph
        .adjacency()
        .iter()
        .map(|edge| CompiledMovement::new(edge.from(), edge.to(), edge.junction_id()))
        .collect();
    MovementTable::new(lane_graph.node_count(), movements)
        .map_err(|_| CompileError::new(CompileErrorCode::MovementInvariant, Vec::new()))
}

struct MovementCurveWork {
    movement: CompiledMovement,
    polyline: Vec<Point2Meters>,
    width_m: f64,
}

fn compile_movement_geometry(
    lanes: &LaneTable,
    origins: &[LaneOrigin],
    movements: &MovementTable,
    options: CompileOptions,
) -> Result<MovementGeometryTable, CompileError> {
    let mut curves = Vec::with_capacity(movements.movements().len());
    let mut work = Vec::with_capacity(movements.movements().len());
    let mut total_points = 0_u64;
    for (index, movement) in movements.movements().iter().copied().enumerate() {
        let movement_id = CompiledMovementId::new(index as u32);
        let source = lanes
            .lane(movement.from())
            .expect("MovementTable validates source lane IDs");
        let destination = lanes
            .lane(movement.to())
            .expect("MovementTable validates target lane IDs");
        let source_direction = (
            source.end().x_m() - source.start().x_m(),
            source.end().y_m() - source.start().y_m(),
        );
        let destination_direction = (
            destination.end().x_m() - destination.start().x_m(),
            destination.end().y_m() - destination.start().y_m(),
        );
        let source_length = libm::hypot(source_direction.0, source_direction.1);
        let destination_length = libm::hypot(destination_direction.0, destination_direction.1);
        let cutback = options
            .approach_cutback_m()
            .min(0.5 * source_length)
            .min(0.5 * destination_length);
        if cutback <= options.geometry_context().distance_tolerance_m() {
            return Err(CompileError::new(
                CompileErrorCode::MovementGeometryDegenerate,
                movement_object_refs(movement, origins),
            ));
        }
        let source_tangent = (
            source_direction.0 / source_length,
            source_direction.1 / source_length,
        );
        let destination_tangent = (
            destination_direction.0 / destination_length,
            destination_direction.1 / destination_length,
        );
        let start = CompiledPoint::new(
            source.end().x_m() - source_tangent.0 * cutback,
            source.end().y_m() - source_tangent.1 * cutback,
        );
        let end = CompiledPoint::new(
            destination.start().x_m() + destination_tangent.0 * cutback,
            destination.start().y_m() + destination_tangent.1 * cutback,
        );
        let compiled_curve =
            CompiledMovementCurve::new(movement_id, start, source.end(), destination.start(), end);
        let geometry_curve = CubicBezier2::new(
            geometry_point(start),
            geometry_point(source.end()),
            geometry_point(destination.start()),
            geometry_point(end),
        );
        let polyline = tessellate_cubic(
            geometry_curve,
            options.movement_tessellation(),
            options.geometry_context(),
        )
        .map_err(|_| {
            CompileError::new(
                CompileErrorCode::MovementGeometryInvariant,
                movement_object_refs(movement, origins),
            )
        })?;
        total_points = total_points
            .checked_add(polyline.len() as u64)
            .ok_or_else(|| {
                CompileError::new(
                    CompileErrorCode::MovementGeometryLimitExceeded,
                    movement_object_refs(movement, origins),
                )
            })?;
        if total_points > options.max_total_movement_points() {
            return Err(CompileError::new(
                CompileErrorCode::MovementGeometryLimitExceeded,
                movement_object_refs(movement, origins),
            ));
        }
        curves.push(compiled_curve);
        work.push(MovementCurveWork {
            movement,
            polyline,
            width_m: source.width_m().min(destination.width_m()),
        });
    }
    let conflict_zones = compile_conflict_zones(&work, origins, options)?;
    MovementGeometryTable::new(movements.movements().len() as u32, curves, conflict_zones)
        .map_err(|_| CompileError::new(CompileErrorCode::MovementGeometryInvariant, Vec::new()))
}

fn compile_conflict_zones(
    work: &[MovementCurveWork],
    origins: &[LaneOrigin],
    options: CompileOptions,
) -> Result<Vec<CompiledConflictZone>, CompileError> {
    let mut zones = Vec::new();
    let mut tested_segments = 0_u64;
    let mut by_junction = BTreeMap::<_, Vec<usize>>::new();
    for (index, movement) in work.iter().enumerate() {
        by_junction
            .entry(movement.movement.junction_id())
            .or_default()
            .push(index);
    }
    for movement_indices in by_junction.values() {
        for first_position in 0..movement_indices.len() {
            for second_position in first_position + 1..movement_indices.len() {
                let first_index = movement_indices[first_position];
                let second_index = movement_indices[second_position];
                let first = &work[first_index];
                let second = &work[second_index];
                let pair_tests = (first.polyline.len().saturating_sub(1) as u64)
                    .checked_mul(second.polyline.len().saturating_sub(1) as u64)
                    .ok_or_else(|| conflict_limit_error(first, second, origins))?;
                tested_segments = tested_segments
                    .checked_add(pair_tests)
                    .ok_or_else(|| conflict_limit_error(first, second, origins))?;
                if tested_segments > options.max_conflict_segment_tests() {
                    return Err(conflict_limit_error(first, second, origins));
                }

                let mut bounds: Option<(f64, f64, f64, f64)> = None;
                let mut kind = CompiledConflictKind::Crossing;
                for first_segment in first.polyline.windows(2) {
                    for second_segment in second.polyline.windows(2) {
                        let intersection = segment_intersection(
                            Segment2::new(first_segment[0], first_segment[1]),
                            Segment2::new(second_segment[0], second_segment[1]),
                            options.geometry_context(),
                        )
                        .map_err(|_| {
                            CompileError::new(
                                CompileErrorCode::MovementGeometryInvariant,
                                conflict_object_refs(first, second, origins),
                            )
                        })?;
                        match intersection {
                            SegmentIntersection::None => {}
                            SegmentIntersection::Point(point) => update_bounds(&mut bounds, point),
                            SegmentIntersection::Overlap(segment) => {
                                kind = CompiledConflictKind::Overlap;
                                update_bounds(&mut bounds, segment.start());
                                update_bounds(&mut bounds, segment.end());
                            }
                        }
                    }
                }
                if let Some((min_x, min_y, max_x, max_y)) = bounds {
                    let half_width = 0.5 * first.width_m.min(second.width_m);
                    let zone = CompiledConflictZone::new(
                        CompiledMovementId::new(first_index as u32),
                        CompiledMovementId::new(second_index as u32),
                        kind,
                        CompiledPoint::new(min_x - half_width, min_y - half_width),
                        CompiledPoint::new(max_x + half_width, max_y + half_width),
                    )
                    .map_err(|_| {
                        CompileError::new(
                            CompileErrorCode::MovementGeometryInvariant,
                            conflict_object_refs(first, second, origins),
                        )
                    })?;
                    zones.push(zone);
                }
            }
        }
    }
    Ok(zones)
}

fn update_bounds(bounds: &mut Option<(f64, f64, f64, f64)>, point: Point2Meters) {
    let x = point.x_m().get();
    let y = point.y_m().get();
    *bounds = Some(match *bounds {
        Some((min_x, min_y, max_x, max_y)) => {
            (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
        }
        None => (x, y, x, y),
    });
}

fn geometry_point(point: CompiledPoint) -> Point2Meters {
    Point2Meters::new(
        CoordinateMeters::try_new(point.x_m()).expect("LaneTable validates finite coordinates"),
        CoordinateMeters::try_new(point.y_m()).expect("LaneTable validates finite coordinates"),
    )
}

fn movement_object_refs(movement: CompiledMovement, origins: &[LaneOrigin]) -> Vec<ObjectRef> {
    let source = origins[movement.from().get() as usize];
    let destination = origins[movement.to().get() as usize];
    vec![
        movement.junction_id().into(),
        source.corridor_id().into(),
        source.lane_id().into(),
        destination.corridor_id().into(),
        destination.lane_id().into(),
    ]
}

fn conflict_object_refs(
    first: &MovementCurveWork,
    second: &MovementCurveWork,
    origins: &[LaneOrigin],
) -> Vec<ObjectRef> {
    let mut refs = movement_object_refs(first.movement, origins);
    refs.extend(movement_object_refs(second.movement, origins));
    refs
}

fn conflict_limit_error(
    first: &MovementCurveWork,
    second: &MovementCurveWork,
    origins: &[LaneOrigin],
) -> CompileError {
    CompileError::new(
        CompileErrorCode::MovementConflictLimitExceeded,
        conflict_object_refs(first, second, origins),
    )
}

fn compile_pedestrian_graph(project: &Project) -> Result<PedestrianGraph, CompileError> {
    let mut origins: Vec<_> = project
        .design_catalog()
        .walking_areas()
        .iter()
        .map(|area| PedestrianNodeOrigin::WalkingArea(area.id()))
        .collect();
    origins.extend(
        project
            .design_catalog()
            .sidewalks()
            .iter()
            .map(|sidewalk| PedestrianNodeOrigin::Sidewalk(sidewalk.id())),
    );
    if origins.len() > u32::MAX as usize {
        return Err(CompileError::new(
            CompileErrorCode::PedestrianGraphInvariant,
            Vec::new(),
        ));
    }
    let walking_nodes: BTreeMap<_, _> = origins
        .iter()
        .enumerate()
        .filter_map(|(index, origin)| match origin {
            PedestrianNodeOrigin::WalkingArea(id) => {
                Some((*id, CompiledPedestrianNodeId::new(index as u32)))
            }
            PedestrianNodeOrigin::Sidewalk(_) => None,
        })
        .collect();
    let mut adjacency = Vec::new();
    for crossing in project.design_catalog().crossings() {
        let from = walking_nodes[&crossing.from()];
        let to = walking_nodes[&crossing.to()];
        if adjacency.len() > MAX_GRAPH_EDGES.saturating_sub(2) {
            return Err(CompileError::new(
                CompileErrorCode::PedestrianGraphInvariant,
                vec![crossing.id().into()],
            ));
        }
        adjacency.push(PedestrianAdjacency::new(from, to, crossing.id()));
        adjacency.push(PedestrianAdjacency::new(to, from, crossing.id()));
    }
    PedestrianGraph::new(origins, adjacency)
        .map_err(|_| CompileError::new(CompileErrorCode::PedestrianGraphInvariant, Vec::new()))
}

fn validate_demand_reachability(
    project: &Project,
    lane_origins: &[LaneOrigin],
    lane_graph: &LaneGraph,
    pedestrian_graph: &PedestrianGraph,
) -> Result<(), CompileError> {
    for profile in project.study_catalog().demand_profiles() {
        for flow in profile.flows() {
            let reachable = match (flow.mode(), flow.origin(), flow.destination()) {
                (
                    DemandMode::Car | DemandMode::Bus,
                    DemandEndpoint::Corridor(from),
                    DemandEndpoint::Corridor(to),
                ) => corridor_reachable(from, to, lane_origins, lane_graph),
                (
                    DemandMode::Pedestrian,
                    DemandEndpoint::WalkingArea(from),
                    DemandEndpoint::WalkingArea(to),
                ) => {
                    let from = pedestrian_graph
                        .node_for_walking_area(from)
                        .expect("StudyCatalog validates walking-area endpoints");
                    let to = pedestrian_graph
                        .node_for_walking_area(to)
                        .expect("StudyCatalog validates walking-area endpoints");
                    pedestrian_graph.can_reach(from, to)
                }
                (DemandMode::Tram, _, _) => {
                    return Err(CompileError::new(
                        CompileErrorCode::DemandModeUnsupported,
                        vec![
                            flow.id().into(),
                            flow.origin().object_ref(),
                            flow.destination().object_ref(),
                        ],
                    ));
                }
                _ => false,
            };
            if !reachable {
                return Err(CompileError::new(
                    CompileErrorCode::DemandEndpointUnreachable,
                    vec![
                        flow.id().into(),
                        flow.origin().object_ref(),
                        flow.destination().object_ref(),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn corridor_reachable(
    from: roadsim_types::CorridorId,
    to: roadsim_types::CorridorId,
    origins: &[LaneOrigin],
    graph: &LaneGraph,
) -> bool {
    let mut from_lanes = origins.iter().enumerate().filter_map(|(index, origin)| {
        (origin.corridor_id() == from).then_some(CompiledLaneId::new(index as u32))
    });
    let to_lanes: Vec<_> = origins
        .iter()
        .enumerate()
        .filter_map(|(index, origin)| {
            (origin.corridor_id() == to).then_some(CompiledLaneId::new(index as u32))
        })
        .collect();
    from_lanes.any(|source| {
        to_lanes
            .iter()
            .any(|destination| graph.can_reach(source, *destination))
    })
}

fn content_hash(
    project: &Project,
    lanes: &LaneTable,
    origins: &[LaneOrigin],
    topology: &CompiledTopology,
    requirements: &CapabilityRequirements,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(COMPILED_NETWORK_SCHEMA_VERSION.to_le_bytes());
    hash.update(project.id().as_uuid().as_bytes());
    hash.update((lanes.len() as u64).to_le_bytes());
    for (lane, origin) in lanes.iter().zip(origins) {
        hash.update(lane.start().x_m().to_bits().to_le_bytes());
        hash.update(lane.start().y_m().to_bits().to_le_bytes());
        hash.update(lane.end().x_m().to_bits().to_le_bytes());
        hash.update(lane.end().y_m().to_bits().to_le_bytes());
        hash.update(lane.width_m().to_bits().to_le_bytes());
        hash.update([lane_use_tag(lane.use_kind())]);
        hash.update(origin.corridor_id().as_uuid().as_bytes());
        hash.update(origin.lane_id().as_uuid().as_bytes());
    }
    hash.update((topology.lane_graph().adjacency().len() as u64).to_le_bytes());
    for edge in topology.lane_graph().adjacency() {
        hash.update(edge.from().get().to_le_bytes());
        hash.update(edge.to().get().to_le_bytes());
        hash.update(edge.junction_id().as_uuid().as_bytes());
    }
    hash.update((topology.movements().movements().len() as u64).to_le_bytes());
    for movement in topology.movements().movements() {
        hash.update(movement.from().get().to_le_bytes());
        hash.update(movement.to().get().to_le_bytes());
        hash.update(movement.junction_id().as_uuid().as_bytes());
    }
    hash.update((topology.movement_geometry().curves().len() as u64).to_le_bytes());
    for curve in topology.movement_geometry().curves() {
        hash.update(curve.movement_id().get().to_le_bytes());
        for point in [
            curve.start(),
            curve.control_1(),
            curve.control_2(),
            curve.end(),
        ] {
            hash.update(point.x_m().to_bits().to_le_bytes());
            hash.update(point.y_m().to_bits().to_le_bytes());
        }
    }
    hash.update((topology.movement_geometry().conflict_zones().len() as u64).to_le_bytes());
    for zone in topology.movement_geometry().conflict_zones() {
        hash.update(zone.first().get().to_le_bytes());
        hash.update(zone.second().get().to_le_bytes());
        hash.update([match zone.kind() {
            CompiledConflictKind::Crossing => 0,
            CompiledConflictKind::Overlap => 1,
        }]);
        hash.update(zone.min().x_m().to_bits().to_le_bytes());
        hash.update(zone.min().y_m().to_bits().to_le_bytes());
        hash.update(zone.max().x_m().to_bits().to_le_bytes());
        hash.update(zone.max().y_m().to_bits().to_le_bytes());
    }
    hash.update((topology.pedestrian_graph().origins().len() as u64).to_le_bytes());
    for origin in topology.pedestrian_graph().origins() {
        match origin {
            PedestrianNodeOrigin::WalkingArea(id) => {
                hash.update([0]);
                hash.update(id.as_uuid().as_bytes());
            }
            PedestrianNodeOrigin::Sidewalk(id) => {
                hash.update([1]);
                hash.update(id.as_uuid().as_bytes());
            }
        }
    }
    hash.update((topology.pedestrian_graph().adjacency().len() as u64).to_le_bytes());
    for edge in topology.pedestrian_graph().adjacency() {
        hash.update(edge.from().get().to_le_bytes());
        hash.update(edge.to().get().to_le_bytes());
        hash.update(edge.crossing_id().as_uuid().as_bytes());
    }
    for capability in requirements.values() {
        hash.update([capability_tag(*capability)]);
    }
    Sha256Digest::from_bytes(hash.finalize().into())
}

const fn lane_use_tag(value: CompiledLaneUse) -> u8 {
    match value {
        CompiledLaneUse::GeneralTraffic => 0,
        CompiledLaneUse::BusOnly => 1,
        CompiledLaneUse::Bicycle => 2,
        CompiledLaneUse::Parking => 3,
    }
}

const fn capability_tag(value: CapabilityId) -> u8 {
    match value {
        CapabilityId::RoadVehiclesBasic => 0,
        CapabilityId::TransitBusLanes => 1,
        CapabilityId::BicycleLanes => 2,
        CapabilityId::ParkingLanes => 3,
        CapabilityId::PedestrianWalkingAreasBasic => 4,
    }
}
