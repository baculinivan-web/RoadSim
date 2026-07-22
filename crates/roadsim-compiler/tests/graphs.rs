use proptest::prelude::*;
use roadsim_compiled_network::{
    CapabilityId, CompiledLaneId, CompiledMovementId, CompiledNetwork, LaneAdjacency, LaneGraph,
    PedestrianNodeOrigin, SourceRevision,
};
use roadsim_compiler::{
    CompileError, CompileErrorCode, CompileOptions, GeometryContext, TessellationOptions,
    compile_project as compile_project_with_options,
};
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CorridorEnd, CorridorEndpointRef,
    CorridorSide, CrossSectionLayout, CrossSectionProfile, CrossSectionSection, Crossing,
    CrsDefinition, CrsProvenance, DemandEndpoint, DemandFlow, DemandInterval, DemandMode,
    DemandProfile, DesignCatalog, EngineeringCrsDescriptor, EngineeringUnit, Junction,
    LaneDefinition, LaneDirection, LaneSlice, LaneUse, LocalOrigin, Point2Meters, Project,
    ProjectMetadata, ReferenceLine, ReferenceLineElement, ReferenceLinePose, Sidewalk,
    StudyCatalog, VerticalDatum, WalkingArea,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, CrossingId, DemandFlowId, DemandProfileId, DurationSeconds,
    FlowRatePerHour, HeadingRadians, JunctionId, LaneId, LengthMeters, ProjectId, SidewalkId,
    WalkingAreaId,
};

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}

fn compile_options() -> CompileOptions {
    CompileOptions::new(
        GeometryContext::new(1.0e-9, 1.0e-12, 1.0e-12, 0.01, 100_000).unwrap(),
        TessellationOptions::new(0.01, 16, 10_000).unwrap(),
        8.0,
        1_000_000,
        1_000_000,
    )
    .unwrap()
}

fn compile_project(
    project: &Project,
    source_revision: SourceRevision,
) -> Result<CompiledNetwork, CompileError> {
    compile_project_with_options(project, source_revision, compile_options())
}

fn point(x: f64, y: f64) -> Point2Meters {
    Point2Meters::new(
        CoordinateMeters::try_new(x).unwrap(),
        CoordinateMeters::try_new(y).unwrap(),
    )
}

fn coordinates() -> CoordinateReference {
    CoordinateReference::new(
        CrsDefinition::Authority(AuthorityCrs::new("EPSG", "4326").unwrap()),
        EngineeringCrsDescriptor::new(
            CrsDefinition::Authority(AuthorityCrs::new("EPSG", "32637").unwrap()),
            EngineeringUnit::Metre,
        )
        .unwrap(),
        LocalOrigin::new(
            CoordinateMeters::try_new(0.0).unwrap(),
            CoordinateMeters::try_new(0.0).unwrap(),
        ),
        AxisOrder::EastNorth,
        VerticalDatum::not_specified(),
        CrsProvenance::new("fixture", "identity", "east-north").unwrap(),
    )
}

fn corridor(id: u128, lane_id: u128, start_x: f64) -> Corridor {
    corridor_with_lanes(id, &[lane_id], start_x)
}

fn corridor_with_lanes(id: u128, lane_ids: &[u128], start_x: f64) -> Corridor {
    oriented_corridor(id, lane_ids, start_x, 0.0, 0.0)
}

fn oriented_corridor(
    id: u128,
    lane_ids: &[u128],
    start_x: f64,
    start_y: f64,
    heading_rad: f64,
) -> Corridor {
    let lane_ids: Vec<_> = lane_ids.iter().copied().map(LaneId::from_u128).collect();
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            point(start_x, start_y),
            HeadingRadians::try_new(heading_rad).unwrap(),
        ),
        vec![ReferenceLineElement::line(length(50.0)).unwrap()],
    )
    .unwrap();
    Corridor::new(
        CorridorId::from_u128(id),
        reference_line,
        lane_ids
            .iter()
            .copied()
            .map(|lane_id| {
                LaneDefinition::new(
                    lane_id,
                    LaneDirection::AlongReference,
                    LaneUse::GeneralTraffic,
                )
            })
            .collect(),
        CrossSectionProfile::new(vec![CrossSectionSection::new(
            length(0.0),
            CrossSectionLayout::new(
                vec![],
                lane_ids
                    .iter()
                    .copied()
                    .map(|lane_id| LaneSlice::new(lane_id, length(3.5)).unwrap())
                    .collect(),
            )
            .unwrap(),
        )])
        .unwrap(),
    )
    .unwrap()
}

fn walking_area(id: u128, x: f64) -> WalkingArea {
    WalkingArea::new(
        WalkingAreaId::from_u128(id),
        vec![point(x, 2.0), point(x + 2.0, 2.0), point(x + 1.0, 4.0)],
    )
    .unwrap()
}

fn flow(
    id: u128,
    mode: DemandMode,
    origin: DemandEndpoint,
    destination: DemandEndpoint,
) -> DemandFlow {
    DemandFlow::new(
        DemandFlowId::from_u128(id),
        mode,
        origin,
        destination,
        vec![
            DemandInterval::new(
                DurationSeconds::try_new(0.0).unwrap(),
                DurationSeconds::try_new(60.0).unwrap(),
                FlowRatePerHour::try_new(60.0).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

fn project(design: DesignCatalog, flow: DemandFlow) -> Project {
    Project::with_catalog(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Graph fixture").unwrap(),
        coordinates(),
        design,
    )
    .with_study_catalog(
        StudyCatalog::new(
            vec![DemandProfile::new(DemandProfileId::from_u128(90), vec![flow]).unwrap()],
            vec![],
            vec![],
        )
        .unwrap(),
    )
    .unwrap()
}

fn project_without_demand(design: DesignCatalog) -> Project {
    Project::with_catalog(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Graph fixture").unwrap(),
        coordinates(),
        design,
    )
}

#[test]
fn connected_corridors_compile_to_directed_lane_reachability() {
    let design = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0), corridor(11, 21, 50.0)],
        vec![
            Junction::new(
                JunctionId::from_u128(40),
                vec![
                    CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                    CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                ],
            )
            .unwrap(),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let network = compile_project(
        &project(
            design,
            flow(
                91,
                DemandMode::Car,
                DemandEndpoint::Corridor(CorridorId::from_u128(10)),
                DemandEndpoint::Corridor(CorridorId::from_u128(11)),
            ),
        ),
        SourceRevision::new(1),
    )
    .unwrap();

    assert_eq!(network.lane_graph().adjacency().len(), 1);
    assert_eq!(network.movements().movements().len(), 1);
    assert_eq!(
        network
            .movements()
            .movement(CompiledMovementId::new(0))
            .unwrap()
            .junction_id(),
        JunctionId::from_u128(40)
    );
    assert_eq!(
        network.lane_graph().adjacency()[0].junction_id(),
        JunctionId::from_u128(40)
    );
    assert!(
        network
            .lane_graph()
            .can_reach(CompiledLaneId::new(0), CompiledLaneId::new(1))
    );
    assert!(
        !network
            .lane_graph()
            .can_reach(CompiledLaneId::new(1), CompiledLaneId::new(0))
    );
}

#[test]
fn one_lane_approaches_infer_stable_merge_and_diverge_movements() {
    let design = DesignCatalog::with_multimodal(
        vec![
            corridor(10, 20, 0.0),
            corridor(11, 21, 50.0),
            corridor(12, 22, 50.0),
            corridor(13, 23, 0.0),
        ],
        vec![
            Junction::new(
                JunctionId::from_u128(40),
                vec![
                    CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                    CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                    CorridorEndpointRef::new(CorridorId::from_u128(12), CorridorEnd::Start),
                    CorridorEndpointRef::new(CorridorId::from_u128(13), CorridorEnd::End),
                ],
            )
            .unwrap(),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let network = compile_project(&project_without_demand(design), SourceRevision::new(1)).unwrap();

    let movements = network.movements().movements();
    assert_eq!(movements.len(), 4);
    assert_eq!(movements[0].from(), CompiledLaneId::new(0));
    assert_eq!(movements[0].to(), CompiledLaneId::new(1));
    assert_eq!(movements[1].from(), CompiledLaneId::new(0));
    assert_eq!(movements[1].to(), CompiledLaneId::new(2));
    assert_eq!(movements[2].from(), CompiledLaneId::new(3));
    assert_eq!(movements[2].to(), CompiledLaneId::new(1));
    assert_eq!(movements[3].from(), CompiledLaneId::new(3));
    assert_eq!(movements[3].to(), CompiledLaneId::new(2));
    assert_eq!(network.movement_geometry().curves().len(), 4);
}

#[test]
fn perpendicular_movements_compile_curves_and_crossing_matrix() {
    let design = DesignCatalog::with_multimodal(
        vec![
            oriented_corridor(10, &[20], -50.0, 0.0, 0.0),
            oriented_corridor(11, &[21], 0.0, 0.0, 0.0),
            oriented_corridor(12, &[22], 0.0, -50.0, std::f64::consts::FRAC_PI_2),
            oriented_corridor(13, &[23], 0.0, 0.0, std::f64::consts::FRAC_PI_2),
        ],
        vec![
            Junction::new(
                JunctionId::from_u128(40),
                vec![
                    CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                    CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                    CorridorEndpointRef::new(CorridorId::from_u128(12), CorridorEnd::End),
                    CorridorEndpointRef::new(CorridorId::from_u128(13), CorridorEnd::Start),
                ],
            )
            .unwrap(),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let project = project_without_demand(design.clone());
    let network = compile_project(&project, SourceRevision::new(1)).unwrap();
    let geometry = network.movement_geometry();

    assert_eq!(geometry.curves().len(), 4);
    let west_to_east = geometry.curve(CompiledMovementId::new(0)).unwrap();
    assert_eq!(west_to_east.start().x_m(), -8.0);
    assert_eq!(west_to_east.start().y_m(), -1.75);
    assert_eq!(west_to_east.end().x_m(), 8.0);
    assert_eq!(west_to_east.end().y_m(), -1.75);
    assert!(geometry.conflicts(CompiledMovementId::new(0), CompiledMovementId::new(3)));
    let crossing = geometry
        .conflict_zones()
        .iter()
        .find(|zone| {
            zone.first() == CompiledMovementId::new(0)
                && zone.second() == CompiledMovementId::new(3)
        })
        .unwrap();
    assert!(crossing.min().x_m() < 1.75 && crossing.max().x_m() > 1.75);
    assert!(crossing.min().y_m() < -1.75 && crossing.max().y_m() > -1.75);

    let repeated = compile_project_with_options(
        &project_without_demand(design.clone()),
        SourceRevision::new(99),
        compile_options(),
    )
    .unwrap();
    assert_eq!(network.movement_geometry(), repeated.movement_geometry());

    let base_options = compile_options();
    let shorter_cutback = CompileOptions::new(
        base_options.geometry_context(),
        base_options.movement_tessellation(),
        4.0,
        base_options.max_total_movement_points(),
        base_options.max_conflict_segment_tests(),
    )
    .unwrap();
    let shorter = compile_project_with_options(
        &project_without_demand(design.clone()),
        SourceRevision::new(1),
        shorter_cutback,
    )
    .unwrap();
    assert_ne!(
        network.header().content_hash(),
        shorter.header().content_hash()
    );
    assert_eq!(
        shorter
            .movement_geometry()
            .curve(CompiledMovementId::new(0))
            .unwrap()
            .start()
            .x_m(),
        -4.0
    );

    let constrained = CompileOptions::new(
        base_options.geometry_context(),
        base_options.movement_tessellation(),
        base_options.approach_cutback_m(),
        base_options.max_total_movement_points(),
        1,
    )
    .unwrap();
    let error = compile_project_with_options(
        &project_without_demand(design),
        SourceRevision::new(1),
        constrained,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        CompileErrorCode::MovementConflictLimitExceeded
    );
    assert!(
        error
            .object_refs()
            .contains(&JunctionId::from_u128(40).into())
    );

    let point_limited = CompileOptions::new(
        base_options.geometry_context(),
        base_options.movement_tessellation(),
        base_options.approach_cutback_m(),
        2,
        base_options.max_conflict_segment_tests(),
    )
    .unwrap();
    let error = compile_project_with_options(
        &project_without_demand(
            DesignCatalog::with_multimodal(
                vec![
                    oriented_corridor(10, &[20], -50.0, 0.0, 0.0),
                    oriented_corridor(11, &[21], 0.0, 0.0, std::f64::consts::FRAC_PI_2),
                ],
                vec![
                    Junction::new(
                        JunctionId::from_u128(40),
                        vec![
                            CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                            CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                        ],
                    )
                    .unwrap(),
                ],
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .unwrap(),
        ),
        SourceRevision::new(1),
        point_limited,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        CompileErrorCode::MovementGeometryLimitExceeded
    );
}

#[test]
fn multiple_target_lanes_on_one_destination_block_ambiguous_movement() {
    let design = DesignCatalog::with_multimodal(
        vec![
            corridor(10, 20, 0.0),
            corridor_with_lanes(11, &[21, 22], 50.0),
        ],
        vec![
            Junction::new(
                JunctionId::from_u128(40),
                vec![
                    CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                    CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                ],
            )
            .unwrap(),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let error =
        compile_project(&project_without_demand(design), SourceRevision::new(1)).unwrap_err();

    assert_eq!(error.code(), CompileErrorCode::MovementAmbiguous);
    assert_eq!(error.object_refs().len(), 6);
    assert!(
        error
            .object_refs()
            .contains(&JunctionId::from_u128(40).into())
    );
    for lane_id in [20, 21, 22] {
        assert!(
            error
                .object_refs()
                .contains(&LaneId::from_u128(lane_id).into())
        );
    }
}

#[test]
fn disconnected_vehicle_demand_blocks_compile_with_flow_and_endpoint_refs() {
    let design = DesignCatalog::new(vec![corridor(10, 20, 0.0), corridor(11, 21, 100.0)]).unwrap();
    let error = compile_project(
        &project(
            design,
            flow(
                91,
                DemandMode::Car,
                DemandEndpoint::Corridor(CorridorId::from_u128(10)),
                DemandEndpoint::Corridor(CorridorId::from_u128(11)),
            ),
        ),
        SourceRevision::new(1),
    )
    .unwrap_err();
    assert_eq!(error.code(), CompileErrorCode::DemandEndpointUnreachable);
    assert_eq!(error.object_refs().len(), 3);
}

#[test]
fn crossing_and_sidewalk_sources_compile_to_pedestrian_graph() {
    let design = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![],
        vec![walking_area(30, 20.0), walking_area(31, 30.0)],
        vec![
            Sidewalk::new(
                SidewalkId::from_u128(50),
                CorridorId::from_u128(10),
                CorridorSide::Right,
                length(0.0),
                length(50.0),
                length(2.0),
            )
            .unwrap(),
        ],
        vec![
            Crossing::new(
                CrossingId::from_u128(60),
                CorridorId::from_u128(10),
                length(25.0),
                length(4.0),
                WalkingAreaId::from_u128(30),
                WalkingAreaId::from_u128(31),
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let network = compile_project(
        &project(
            design,
            flow(
                91,
                DemandMode::Pedestrian,
                DemandEndpoint::WalkingArea(WalkingAreaId::from_u128(30)),
                DemandEndpoint::WalkingArea(WalkingAreaId::from_u128(31)),
            ),
        ),
        SourceRevision::new(1),
    )
    .unwrap();

    assert_eq!(network.pedestrian_graph().origins().len(), 3);
    assert_eq!(network.pedestrian_graph().adjacency().len(), 2);
    assert!(
        network
            .pedestrian_graph()
            .origins()
            .contains(&PedestrianNodeOrigin::Sidewalk(SidewalkId::from_u128(50)))
    );
    assert!(
        network
            .requirements()
            .contains(CapabilityId::PedestrianWalkingAreasBasic)
    );
}

#[test]
fn disconnected_pedestrian_demand_is_rejected_before_backend_compile() {
    let design = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![],
        vec![walking_area(30, 20.0), walking_area(31, 30.0)],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let error = compile_project(
        &project(
            design,
            flow(
                91,
                DemandMode::Pedestrian,
                DemandEndpoint::WalkingArea(WalkingAreaId::from_u128(30)),
                DemandEndpoint::WalkingArea(WalkingAreaId::from_u128(31)),
            ),
        ),
        SourceRevision::new(1),
    )
    .unwrap_err();
    assert_eq!(error.code(), CompileErrorCode::DemandEndpointUnreachable);
    assert_eq!(error.object_refs().len(), 3);
}

proptest! {
    #[test]
    fn directed_chain_reachability_is_stable(node_count in 2_u32..128) {
        let edges = (0..node_count - 1)
            .map(|index| LaneAdjacency::new(
                CompiledLaneId::new(index),
                CompiledLaneId::new(index + 1),
                JunctionId::from_u128(u128::from(index) + 1),
            ))
            .collect();
        let graph = LaneGraph::new(node_count, edges).unwrap();
        prop_assert!(graph.can_reach(
            CompiledLaneId::new(0),
            CompiledLaneId::new(node_count - 1),
        ));
        prop_assert!(!graph.can_reach(
            CompiledLaneId::new(node_count - 1),
            CompiledLaneId::new(0),
        ));
    }

    #[test]
    fn one_lane_diverge_movement_ids_are_stable(outgoing_count in 1_u32..16) {
        let mut corridors = vec![corridor(10, 20, 0.0)];
        let mut approaches = vec![CorridorEndpointRef::new(
            CorridorId::from_u128(10),
            CorridorEnd::End,
        )];
        for index in 0..outgoing_count {
            let corridor_id = u128::from(index) + 100;
            corridors.push(corridor(corridor_id, u128::from(index) + 200, 50.0));
            approaches.push(CorridorEndpointRef::new(
                CorridorId::from_u128(corridor_id),
                CorridorEnd::Start,
            ));
        }
        let design = DesignCatalog::with_multimodal(
            corridors,
            vec![Junction::new(JunctionId::from_u128(40), approaches).unwrap()],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .unwrap();
        let first = compile_project(&project_without_demand(design.clone()), SourceRevision::new(1)).unwrap();
        let second = compile_project(&project_without_demand(design), SourceRevision::new(2)).unwrap();
        prop_assert_eq!(first.movements(), second.movements());
        prop_assert_eq!(first.movements().movements().len(), outgoing_count as usize);
    }
}
