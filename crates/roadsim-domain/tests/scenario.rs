use proptest::prelude::*;
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CrossSectionLayout,
    CrossSectionProfile, CrossSectionSection, CrsDefinition, CrsProvenance, DemandEndpoint,
    DemandFlow, DemandInterval, DemandMode, DemandProfile, DesignCatalog, EngineeringCrsDescriptor,
    EngineeringUnit, Experiment, LaneDefinition, LaneDirection, LaneSlice, LaneUse, LocalOrigin,
    Point2Meters, Project, ProjectMetadata, ReferenceLine, ReferenceLineElement, ReferenceLinePose,
    Scenario, ScenarioErrorCode, ScenarioTiming, StudyCatalog, VerticalDatum,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, DemandFlowId, DemandProfileId, DurationSeconds, ExperimentId,
    FlowRatePerHour, HeadingRadians, LaneId, LengthMeters, ProjectId, RootSeed, ScenarioId,
    WalkingAreaId,
};

fn duration(value: f64) -> DurationSeconds {
    DurationSeconds::try_new(value).unwrap()
}

fn rate(value: f64) -> FlowRatePerHour {
    FlowRatePerHour::try_new(value).unwrap()
}

fn corridor(id: u128, lane_id: u128, x: f64) -> Corridor {
    let lane_id = LaneId::from_u128(lane_id);
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(x).unwrap(),
                CoordinateMeters::try_new(0.0).unwrap(),
            ),
            HeadingRadians::try_new(0.0).unwrap(),
        ),
        vec![ReferenceLineElement::line(LengthMeters::try_new(50.0).unwrap()).unwrap()],
    )
    .unwrap();
    Corridor::new(
        CorridorId::from_u128(id),
        reference_line,
        vec![LaneDefinition::new(
            lane_id,
            LaneDirection::AlongReference,
            LaneUse::GeneralTraffic,
        )],
        CrossSectionProfile::new(vec![CrossSectionSection::new(
            LengthMeters::try_new(0.0).unwrap(),
            CrossSectionLayout::new(
                vec![],
                vec![LaneSlice::new(lane_id, LengthMeters::try_new(3.5).unwrap()).unwrap()],
            )
            .unwrap(),
        )])
        .unwrap(),
    )
    .unwrap()
}

fn base_project() -> Project {
    let source = CrsDefinition::Authority(AuthorityCrs::new("EPSG", "4326").unwrap());
    let engineering = EngineeringCrsDescriptor::new(
        CrsDefinition::Authority(AuthorityCrs::new("EPSG", "32637").unwrap()),
        EngineeringUnit::Metre,
    )
    .unwrap();
    Project::with_catalog(
        ProjectId::from_u128(1),
        ProjectMetadata::new("Scenario test").unwrap(),
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
        DesignCatalog::new(vec![corridor(10, 20, 0.0), corridor(11, 21, 50.0)]).unwrap(),
    )
}

fn interval(start: f64, end: f64, flow: f64) -> DemandInterval {
    DemandInterval::new(duration(start), duration(end), rate(flow)).unwrap()
}

fn study_catalog() -> StudyCatalog {
    let profile_id = DemandProfileId::from_u128(40);
    let scenario_id = ScenarioId::from_u128(50);
    let flow = DemandFlow::new(
        DemandFlowId::from_u128(41),
        DemandMode::Car,
        DemandEndpoint::Corridor(CorridorId::from_u128(10)),
        DemandEndpoint::Corridor(CorridorId::from_u128(11)),
        vec![
            interval(1_800.0, 3_600.0, 600.0),
            interval(0.0, 1_800.0, 500.0),
        ],
    )
    .unwrap();
    let profile = DemandProfile::new(profile_id, vec![flow]).unwrap();
    let timing = ScenarioTiming::new(
        duration(60.0),
        duration(3_600.0),
        duration(1.0),
        duration(10.0),
    )
    .unwrap();
    let scenario = Scenario::new(scenario_id, profile_id, timing, RootSeed::new(42));
    let experiment = Experiment::new(
        ExperimentId::from_u128(60),
        vec![scenario_id],
        vec![RootSeed::new(9), RootSeed::new(7)],
    )
    .unwrap();
    StudyCatalog::new(vec![profile], vec![scenario], vec![experiment]).unwrap()
}

#[test]
fn project_round_trip_preserves_explicit_units_intervals_and_seed_policy() {
    let project = base_project().with_study_catalog(study_catalog()).unwrap();
    let flow = &project.study_catalog().demand_profiles()[0].flows()[0];
    assert_eq!(flow.intervals()[0].start().get(), 0.0);
    assert_eq!(flow.intervals()[0].flow_per_hour().get(), 500.0);
    assert_eq!(
        project.study_catalog().scenarios()[0].root_seed(),
        RootSeed::new(42)
    );
    assert_eq!(
        project.study_catalog().experiments()[0].replication_seeds(),
        &[RootSeed::new(7), RootSeed::new(9)]
    );

    let encoded = serde_json::to_string(&project).unwrap();
    let decoded: Project = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, project);
}

#[test]
fn flow_rejects_overlap_and_mode_endpoint_confusion() {
    let id = DemandFlowId::from_u128(41);
    let error = DemandFlow::new(
        id,
        DemandMode::Car,
        DemandEndpoint::Corridor(CorridorId::from_u128(10)),
        DemandEndpoint::Corridor(CorridorId::from_u128(11)),
        vec![interval(0.0, 100.0, 500.0), interval(99.0, 200.0, 600.0)],
    )
    .unwrap_err();
    assert_eq!(error.code(), ScenarioErrorCode::OverlappingIntervals);
    assert_eq!(error.object_refs(), &[id.into()]);

    let error = DemandFlow::new(
        id,
        DemandMode::Pedestrian,
        DemandEndpoint::Corridor(CorridorId::from_u128(10)),
        DemandEndpoint::WalkingArea(WalkingAreaId::from_u128(30)),
        vec![interval(0.0, 100.0, 100.0)],
    )
    .unwrap_err();
    assert_eq!(error.code(), ScenarioErrorCode::IncompatibleEndpoint);
    assert_eq!(error.object_refs().len(), 3);
}

#[test]
fn timing_parameters_are_distinct_and_revalidated_on_deserialization() {
    assert_eq!(
        ScenarioTiming::new(
            duration(60.0),
            duration(60.0),
            duration(1.0),
            duration(10.0),
        )
        .unwrap_err()
        .code(),
        ScenarioErrorCode::WarmupOutsideDuration
    );
    assert_eq!(
        ScenarioTiming::new(duration(0.0), duration(60.0), duration(2.0), duration(1.0),)
            .unwrap_err()
            .code(),
        ScenarioErrorCode::SamplingFasterThanStep
    );

    let invalid = serde_json::json!({
        "warmup_s": 0.0,
        "duration_s": 60.0,
        "step_s": 0.0,
        "output_sampling_s": 10.0
    });
    assert!(serde_json::from_value::<ScenarioTiming>(invalid).is_err());
}

#[test]
fn project_rejects_dangling_demand_and_scenario_references() {
    let flow = DemandFlow::new(
        DemandFlowId::from_u128(41),
        DemandMode::Car,
        DemandEndpoint::Corridor(CorridorId::from_u128(10)),
        DemandEndpoint::Corridor(CorridorId::from_u128(999)),
        vec![interval(0.0, 100.0, 500.0)],
    )
    .unwrap();
    let profile = DemandProfile::new(DemandProfileId::from_u128(40), vec![flow]).unwrap();
    let error = base_project()
        .with_study_catalog(StudyCatalog::new(vec![profile], vec![], vec![]).unwrap())
        .unwrap_err();
    assert_eq!(error.code(), ScenarioErrorCode::DanglingDemandEndpoint);
    assert_eq!(error.object_refs().len(), 2);

    let timing =
        ScenarioTiming::new(duration(0.0), duration(100.0), duration(1.0), duration(1.0)).unwrap();
    let scenario = Scenario::new(
        ScenarioId::from_u128(50),
        DemandProfileId::from_u128(999),
        timing,
        RootSeed::new(1),
    );
    let error = base_project()
        .with_study_catalog(StudyCatalog::new(vec![], vec![scenario], vec![]).unwrap())
        .unwrap_err();
    assert_eq!(error.code(), ScenarioErrorCode::DanglingDemandProfile);
    assert_eq!(error.object_refs().len(), 2);
}

proptest! {
    #[test]
    fn adjacent_positive_intervals_are_canonicalized_without_overlap(
        boundary in 1_u16..3_599,
        first_rate in 1_u16..2_000,
        second_rate in 1_u16..2_000,
    ) {
        let boundary = f64::from(boundary);
        let flow = DemandFlow::new(
            DemandFlowId::from_u128(41),
            DemandMode::Car,
            DemandEndpoint::Corridor(CorridorId::from_u128(10)),
            DemandEndpoint::Corridor(CorridorId::from_u128(11)),
            vec![
                interval(boundary, 3_600.0, f64::from(second_rate)),
                interval(0.0, boundary, f64::from(first_rate)),
            ],
        )
        .unwrap();

        prop_assert_eq!(flow.intervals()[0].start().get(), 0.0);
        prop_assert_eq!(flow.intervals()[0].end().get(), boundary);
        prop_assert_eq!(flow.intervals()[1].start().get(), boundary);
    }
}
