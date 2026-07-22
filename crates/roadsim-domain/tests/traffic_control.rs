use proptest::prelude::*;
use roadsim_domain::{
    CatalogErrorCode, ControlCode, ControlErrorCode, Corridor, CorridorEnd, CorridorEndpointRef,
    CrossSectionLayout, CrossSectionProfile, CrossSectionSection, DesignCatalog, GroupState,
    Junction, LaneDefinition, LaneDirection, LaneSlice, LaneUse, Point2Meters, ReferenceLine,
    ReferenceLineElement, ReferenceLinePose, RoadMarking, SignalController, SignalGroup,
    SignalHead, SignalIndication, SignalMovementBinding, SignalPhase, SignalProgram, StopLine,
    TrafficControlCatalog, TrafficSign,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, DurationSeconds, HeadingRadians, JunctionId, LaneId,
    LengthMeters, RoadMarkingId, SignalControllerId, SignalGroupId, SignalHeadId, SignalPhaseId,
    SignalProgramId, StopLineId, TrafficSignId,
};

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}

fn duration(value: f64) -> DurationSeconds {
    DurationSeconds::try_new(value).unwrap()
}

fn corridor(id: u128, lane_id: u128, x: f64) -> Corridor {
    let lane_id = LaneId::from_u128(lane_id);
    let start = Point2Meters::new(
        CoordinateMeters::try_new(x).unwrap(),
        CoordinateMeters::try_new(0.0).unwrap(),
    );
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(start, HeadingRadians::try_new(0.0).unwrap()),
        vec![ReferenceLineElement::line(length(50.0)).unwrap()],
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
            length(0.0),
            CrossSectionLayout::new(vec![], vec![LaneSlice::new(lane_id, length(3.5)).unwrap()])
                .unwrap(),
        )])
        .unwrap(),
    )
    .unwrap()
}

fn multimodal_catalog() -> DesignCatalog {
    DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0), corridor(11, 21, 50.0)],
        vec![
            Junction::new(
                JunctionId::from_u128(30),
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
    .unwrap()
}

fn controls() -> TrafficControlCatalog {
    let junction_id = JunctionId::from_u128(30);
    let group_id = SignalGroupId::from_u128(40);
    let program_id = SignalProgramId::from_u128(44);
    TrafficControlCatalog::new(
        vec![TrafficSign::new(
            TrafficSignId::from_u128(50),
            CorridorId::from_u128(10),
            length(10.0),
            ControlCode::new("priority").unwrap(),
        )],
        vec![
            RoadMarking::new(
                RoadMarkingId::from_u128(51),
                CorridorId::from_u128(10),
                length(5.0),
                length(15.0),
                vec![LaneId::from_u128(20)],
                ControlCode::new("lane_arrow").unwrap(),
            )
            .unwrap(),
        ],
        vec![
            StopLine::new(
                StopLineId::from_u128(52),
                CorridorId::from_u128(10),
                length(45.0),
                vec![LaneId::from_u128(20)],
            )
            .unwrap(),
        ],
        vec![SignalGroup::new(group_id, junction_id)],
        vec![SignalHead::new(
            SignalHeadId::from_u128(41),
            junction_id,
            group_id,
        )],
        vec![
            SignalProgram::new(
                program_id,
                junction_id,
                vec![group_id],
                vec![
                    SignalPhase::new(
                        SignalPhaseId::from_u128(43),
                        duration(30.0),
                        duration(3.0),
                        vec![GroupState::new(group_id, SignalIndication::Green)],
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![
            SignalController::new(
                SignalControllerId::from_u128(45),
                junction_id,
                vec![program_id],
                program_id,
            )
            .unwrap(),
        ],
    )
    .unwrap()
    .with_signal_movement_bindings(vec![
        SignalMovementBinding::new(group_id, LaneId::from_u128(20), LaneId::from_u128(21)).unwrap(),
    ])
    .unwrap()
}

#[test]
fn typical_controls_round_trip_in_deterministic_order() {
    let catalog = multimodal_catalog()
        .with_traffic_controls(controls())
        .unwrap();

    assert_eq!(catalog.traffic_controls().signs().len(), 1);
    assert_eq!(catalog.traffic_controls().markings().len(), 1);
    assert_eq!(catalog.traffic_controls().stop_lines().len(), 1);
    assert_eq!(catalog.traffic_controls().signal_heads().len(), 1);
    assert_eq!(
        catalog.traffic_controls().signal_movement_bindings(),
        &[SignalMovementBinding::new(
            SignalGroupId::from_u128(40),
            LaneId::from_u128(20),
            LaneId::from_u128(21),
        )
        .unwrap()]
    );
    assert_eq!(
        catalog.traffic_controls().signal_programs()[0].phases()[0]
            .duration()
            .get(),
        30.0
    );

    let encoded = serde_json::to_string(&catalog).unwrap();
    let decoded: DesignCatalog = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, catalog);
}

#[test]
fn movement_bindings_are_explicit_sorted_and_revalidated() {
    let first = SignalMovementBinding::new(
        SignalGroupId::from_u128(40),
        LaneId::from_u128(20),
        LaneId::from_u128(21),
    )
    .unwrap();
    let second = SignalMovementBinding::new(
        SignalGroupId::from_u128(40),
        LaneId::from_u128(21),
        LaneId::from_u128(20),
    )
    .unwrap();
    let catalog = TrafficControlCatalog::new(
        vec![],
        vec![],
        vec![],
        vec![SignalGroup::new(
            SignalGroupId::from_u128(40),
            JunctionId::from_u128(30),
        )],
        vec![],
        vec![],
        vec![],
    )
    .unwrap()
    .with_signal_movement_bindings(vec![second, first])
    .unwrap();
    assert_eq!(catalog.signal_movement_bindings(), &[first, second]);

    let duplicate = catalog
        .clone()
        .with_signal_movement_bindings(vec![first, first])
        .unwrap_err();
    assert_eq!(duplicate.code(), ControlErrorCode::DuplicateReference);

    let self_loop = SignalMovementBinding::new(
        SignalGroupId::from_u128(40),
        LaneId::from_u128(20),
        LaneId::from_u128(20),
    )
    .unwrap_err();
    assert_eq!(self_loop.code(), ControlErrorCode::InvalidMovementBinding);
    let mut invalid_wire = serde_json::to_value(first).unwrap();
    invalid_wire["to_lane_id"] = invalid_wire["from_lane_id"].clone();
    assert!(serde_json::from_value::<SignalMovementBinding>(invalid_wire).is_err());

    let dangling = catalog
        .with_signal_movement_bindings(vec![
            SignalMovementBinding::new(
                SignalGroupId::from_u128(40),
                LaneId::from_u128(20),
                LaneId::from_u128(999),
            )
            .unwrap(),
        ])
        .unwrap();
    let error = multimodal_catalog()
        .with_traffic_controls(dangling)
        .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::DanglingLane);
    assert!(error.object_refs().contains(&LaneId::from_u128(999).into()));
}

#[test]
fn phase_and_controller_invariants_have_stable_evidence() {
    let group_id = SignalGroupId::from_u128(40);
    let phase_id = SignalPhaseId::from_u128(43);
    let error = SignalPhase::new(
        phase_id,
        duration(0.0),
        duration(0.0),
        vec![GroupState::new(group_id, SignalIndication::Red)],
    )
    .unwrap_err();
    assert_eq!(error.code(), ControlErrorCode::ZeroPhaseDuration);
    assert_eq!(error.object_refs(), &[phase_id.into()]);

    let program_id = SignalProgramId::from_u128(44);
    let error = SignalController::new(
        SignalControllerId::from_u128(45),
        JunctionId::from_u128(30),
        vec![program_id],
        SignalProgramId::from_u128(999),
    )
    .unwrap_err();
    assert_eq!(error.code(), ControlErrorCode::ActiveProgramMissing);
    assert_eq!(error.object_refs().len(), 2);
}

#[test]
fn signal_program_preserves_authored_phase_sequence() {
    let group_id = SignalGroupId::from_u128(40);
    let make_phase = |id, indication| {
        SignalPhase::new(
            SignalPhaseId::from_u128(id),
            duration(10.0),
            duration(2.0),
            vec![GroupState::new(group_id, indication)],
        )
        .unwrap()
    };
    let program = SignalProgram::new(
        SignalProgramId::from_u128(44),
        JunctionId::from_u128(30),
        vec![group_id],
        vec![
            make_phase(200, SignalIndication::Green),
            make_phase(100, SignalIndication::Red),
        ],
    )
    .unwrap();

    assert_eq!(program.phases()[0].id(), SignalPhaseId::from_u128(200));
    assert_eq!(program.phases()[1].id(), SignalPhaseId::from_u128(100));
}

#[test]
fn aggregate_rejects_dangling_lane_and_cross_junction_signal_group() {
    let marking = RoadMarking::new(
        RoadMarkingId::from_u128(51),
        CorridorId::from_u128(10),
        length(5.0),
        length(15.0),
        vec![LaneId::from_u128(999)],
        ControlCode::new("lane_arrow").unwrap(),
    )
    .unwrap();
    let bad_controls = TrafficControlCatalog::new(
        vec![],
        vec![marking],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let error = multimodal_catalog()
        .with_traffic_controls(bad_controls)
        .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::DanglingLane);
    assert_eq!(error.object_refs().len(), 2);

    let group_id = SignalGroupId::from_u128(40);
    let head_id = SignalHeadId::from_u128(41);
    let bad_controls = TrafficControlCatalog::new(
        vec![],
        vec![],
        vec![],
        vec![SignalGroup::new(group_id, JunctionId::from_u128(30))],
        vec![SignalHead::new(
            head_id,
            JunctionId::from_u128(999),
            group_id,
        )],
        vec![],
        vec![],
    )
    .unwrap();
    let error = multimodal_catalog()
        .with_traffic_controls(bad_controls)
        .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::DanglingJunction);
    assert!(error.object_refs().contains(&head_id.into()));
}

#[test]
fn deserialization_rechecks_catalog_and_phase_invariants() {
    let mut legacy = serde_json::to_value(controls()).unwrap();
    legacy.as_object_mut().unwrap().remove("movement_bindings");
    let decoded: TrafficControlCatalog = serde_json::from_value(legacy).unwrap();
    assert!(decoded.signal_movement_bindings().is_empty());

    let mut value = serde_json::to_value(controls()).unwrap();
    let duplicate = value["signs"][0].clone();
    value["signs"].as_array_mut().unwrap().push(duplicate);
    assert!(serde_json::from_value::<TrafficControlCatalog>(value).is_err());

    let mut value = serde_json::to_value(controls()).unwrap();
    value["programs"][0]["phases"][0]["duration_s"] = serde_json::json!(0.0);
    assert!(serde_json::from_value::<TrafficControlCatalog>(value).is_err());
}

proptest! {
    #[test]
    fn valid_phase_timings_round_trip_without_order_dependence(
        duration_s in 1_u16..3_600,
        intergreen_s in 0_u8..120,
    ) {
        let first = SignalGroupId::from_u128(40);
        let second = SignalGroupId::from_u128(41);
        let phase = SignalPhase::new(
            SignalPhaseId::from_u128(43),
            duration(f64::from(duration_s)),
            duration(f64::from(intergreen_s)),
            vec![
                GroupState::new(second, SignalIndication::Red),
                GroupState::new(first, SignalIndication::Green),
            ],
        )
        .unwrap();
        let program = SignalProgram::new(
            SignalProgramId::from_u128(44),
            JunctionId::from_u128(30),
            vec![second, first],
            vec![phase],
        )
        .unwrap();

        let encoded = serde_json::to_string(&program).unwrap();
        let decoded: SignalProgram = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(decoded.group_ids(), &[first, second]);
        prop_assert_eq!(decoded, program);
    }
}
