use proptest::prelude::*;
use roadsim_compiled_network::{
    CapabilityId, CompiledLaneId, CompiledLaneUse, CompiledNetwork, SourceRevision,
};
use roadsim_compiler::{
    CompileError, CompileErrorCode, CompileOptions, CompileOptionsErrorCode, GeometryContext,
    MAX_CONFLICT_SEGMENT_TESTS, MAX_TOTAL_MOVEMENT_POINTS, TessellationOptions,
    compile_project as compile_project_with_options,
};
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CrossSectionLayout,
    CrossSectionProfile, CrossSectionSection, CrsDefinition, CrsProvenance, DesignCatalog,
    EngineeringCrsDescriptor, EngineeringUnit, LaneDefinition, LaneDirection, LaneSlice, LaneUse,
    LocalOrigin, Point2Meters, Project, ProjectMetadata, ReferenceLine, ReferenceLineElement,
    ReferenceLinePose, VerticalDatum,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, CurvaturePerMeter, HeadingRadians, LaneId, LengthMeters,
    ProjectId,
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

#[test]
fn compile_options_reject_invalid_or_unbounded_movement_policy() {
    let geometry = compile_options().geometry_context();
    let tessellation = compile_options().movement_tessellation();
    assert_eq!(
        CompileOptions::new(geometry, tessellation, 0.0, 2, 1)
            .unwrap_err()
            .code(),
        CompileOptionsErrorCode::InvalidApproachCutback
    );
    assert_eq!(
        CompileOptions::new(
            geometry,
            tessellation,
            1.0,
            2,
            MAX_CONFLICT_SEGMENT_TESTS + 1,
        )
        .unwrap_err()
        .code(),
        CompileOptionsErrorCode::InvalidConflictTestLimit
    );
    assert_eq!(
        CompileOptions::new(
            geometry,
            tessellation,
            1.0,
            MAX_TOTAL_MOVEMENT_POINTS + 1,
            1,
        )
        .unwrap_err()
        .code(),
        CompileOptionsErrorCode::InvalidMovementPointLimit
    );
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

fn project_with_corridor(corridor: Corridor) -> Project {
    Project::with_catalog(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Compiler fixture").unwrap(),
        coordinates(),
        DesignCatalog::new(vec![corridor]).unwrap(),
    )
}

fn straight_corridor(length_m: f64, width_m: f64, heading: f64) -> Corridor {
    let left_id = LaneId::from_u128(11);
    let right_id = LaneId::from_u128(12);
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(10.0).unwrap(),
                CoordinateMeters::try_new(20.0).unwrap(),
            ),
            HeadingRadians::try_new(heading).unwrap(),
        ),
        vec![ReferenceLineElement::line(length(length_m)).unwrap()],
    )
    .unwrap();
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0),
        CrossSectionLayout::new(
            vec![LaneSlice::new(left_id, length(width_m)).unwrap()],
            vec![LaneSlice::new(right_id, length(width_m)).unwrap()],
        )
        .unwrap(),
    )])
    .unwrap();
    Corridor::new(
        CorridorId::from_u128(10),
        reference_line,
        vec![
            LaneDefinition::new(
                left_id,
                LaneDirection::AgainstReference,
                LaneUse::GeneralTraffic,
            ),
            LaneDefinition::new(
                right_id,
                LaneDirection::AlongReference,
                LaneUse::GeneralTraffic,
            ),
        ],
        profile,
    )
    .unwrap()
}

#[test]
fn straight_two_way_corridor_compiles_to_oriented_lane_arrays() {
    let project = project_with_corridor(straight_corridor(100.0, 3.5, 0.0));
    let network = compile_project(&project, SourceRevision::new(7)).unwrap();

    assert_eq!(network.header().schema_version(), 5);
    assert_eq!(network.header().source_revision().get(), 7);
    assert_eq!(network.lanes().len(), 2);
    assert!(
        network
            .requirements()
            .contains(CapabilityId::RoadVehiclesBasic)
    );

    let left = network.lanes().lane(CompiledLaneId::new(0)).unwrap();
    assert_eq!(left.use_kind(), CompiledLaneUse::GeneralTraffic);
    assert_eq!((left.start().x_m(), left.start().y_m()), (110.0, 21.75));
    assert_eq!((left.end().x_m(), left.end().y_m()), (10.0, 21.75));
    assert_eq!(left.length_m(), 100.0);
    assert_eq!(
        network.lane_origin(left.id()).unwrap().lane_id(),
        LaneId::from_u128(11)
    );

    let right = network.lanes().lane(CompiledLaneId::new(1)).unwrap();
    assert_eq!((right.start().x_m(), right.start().y_m()), (10.0, 18.25));
    assert_eq!((right.end().x_m(), right.end().y_m()), (110.0, 18.25));
    assert_eq!(
        network.lane_origin(right.id()).unwrap().lane_id(),
        LaneId::from_u128(12)
    );
}

#[test]
fn semantic_content_hash_is_stable_across_source_revision() {
    let project = project_with_corridor(straight_corridor(100.0, 3.5, 0.0));
    let first = compile_project(&project, SourceRevision::new(1)).unwrap();
    let second = compile_project(&project, SourceRevision::new(99)).unwrap();
    assert_eq!(
        first.header().content_hash(),
        second.header().content_hash()
    );
    assert_eq!(
        first.header().content_hash().to_string(),
        "a943e63414998770155661d9439b2d29ae141958d66818624b379c1ba619198c"
    );
}

#[test]
fn unsupported_geometry_fails_with_design_object_reference() {
    let lane_id = LaneId::from_u128(11);
    let corridor_id = CorridorId::from_u128(10);
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(0.0).unwrap(),
                CoordinateMeters::try_new(0.0).unwrap(),
            ),
            HeadingRadians::try_new(0.0).unwrap(),
        ),
        vec![
            ReferenceLineElement::circular_arc(
                length(20.0),
                CurvaturePerMeter::try_new(0.01).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0),
        CrossSectionLayout::new(vec![], vec![LaneSlice::new(lane_id, length(3.5)).unwrap()])
            .unwrap(),
    )])
    .unwrap();
    let corridor = Corridor::new(
        corridor_id,
        reference_line,
        vec![LaneDefinition::new(
            lane_id,
            LaneDirection::AlongReference,
            LaneUse::GeneralTraffic,
        )],
        profile,
    )
    .unwrap();
    let error =
        compile_project(&project_with_corridor(corridor), SourceRevision::new(0)).unwrap_err();
    assert_eq!(error.code(), CompileErrorCode::UnsupportedReferenceElement);
    assert_eq!(error.object_refs(), &[corridor_id.into()]);
}

#[test]
fn empty_network_is_not_published_as_a_runnable_csn() {
    let project = Project::new(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Empty").unwrap(),
        coordinates(),
    );
    let error = compile_project(&project, SourceRevision::new(0)).unwrap_err();
    assert_eq!(error.code(), CompileErrorCode::EmptyNetwork);
    assert_eq!(error.object_refs(), &[project.id().into()]);
}

proptest! {
    #[test]
    fn compiled_straight_lane_length_matches_design_length(
        corridor_length in 1.0_f64..10_000.0,
        lane_width in 0.5_f64..10.0,
        heading in -100.0_f64..100.0,
    ) {
        let project = project_with_corridor(straight_corridor(corridor_length, lane_width, heading));
        let network = compile_project(&project, SourceRevision::new(0)).unwrap();
        for lane in network.lanes().iter() {
            prop_assert!((lane.length_m() - corridor_length).abs() <= corridor_length * 1e-12);
            prop_assert!(lane.start().x_m().is_finite());
            prop_assert!(lane.start().y_m().is_finite());
            prop_assert!(lane.end().x_m().is_finite());
            prop_assert!(lane.end().y_m().is_finite());
        }
    }
}
