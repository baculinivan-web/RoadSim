use proptest::prelude::*;
use roadsim_compiled_network::{
    CapabilityId, CompiledLaneId, CompiledMovementId, LaneAdjacency, LaneGraph,
    PedestrianNodeOrigin, SourceRevision,
};
use roadsim_compiler::{CompileErrorCode, compile_project};
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
    let lane_ids: Vec<_> = lane_ids.iter().copied().map(LaneId::from_u128).collect();
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(point(start_x, 0.0), HeadingRadians::try_new(0.0).unwrap()),
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
