use proptest::prelude::*;
use roadsim_domain::{
    Corridor, CorridorErrorCode, CrossSectionLayout, CrossSectionProfile, CrossSectionSection,
    DesignCatalog, LaneDefinition, LaneDirection, LaneSlice, LaneUse, Point2Meters, ReferenceLine,
    ReferenceLineElement, ReferenceLinePose,
};
use roadsim_types::{CoordinateMeters, CorridorId, HeadingRadians, LaneId, LengthMeters};

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}
fn lane_id(value: u128) -> LaneId {
    LaneId::from_u128(value)
}
fn reference_line(total: f64) -> ReferenceLine {
    ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(0.0).unwrap(),
                CoordinateMeters::try_new(0.0).unwrap(),
            ),
            HeadingRadians::try_new(0.0).unwrap(),
        ),
        vec![ReferenceLineElement::line(length(total)).unwrap()],
    )
    .unwrap()
}
fn slice(id: u128, width: f64) -> LaneSlice {
    LaneSlice::new(lane_id(id), length(width)).unwrap()
}
fn definition(id: u128, direction: LaneDirection) -> LaneDefinition {
    LaneDefinition::new(lane_id(id), direction, LaneUse::GeneralTraffic)
}

fn typical_corridor() -> Corridor {
    typical_corridor_with_id(10)
}

fn typical_corridor_with_id(id: u128) -> Corridor {
    let sections = CrossSectionProfile::new(vec![
        CrossSectionSection::new(
            length(0.0),
            CrossSectionLayout::new(
                vec![slice(2, 3.5), slice(1, 3.25)],
                vec![slice(3, 3.25), slice(4, 3.5)],
            )
            .unwrap(),
        ),
        CrossSectionSection::new(
            length(60.0),
            CrossSectionLayout::new(
                vec![slice(2, 3.25), slice(1, 3.0)],
                vec![slice(3, 3.0), slice(4, 3.25)],
            )
            .unwrap(),
        ),
    ])
    .unwrap();
    Corridor::new(
        CorridorId::from_u128(id),
        reference_line(100.0),
        vec![
            definition(1, LaneDirection::AgainstReference),
            definition(2, LaneDirection::AgainstReference),
            definition(3, LaneDirection::AlongReference),
            definition(4, LaneDirection::AlongReference),
        ],
        sections,
    )
    .unwrap()
}

#[test]
fn typical_two_way_approach_is_semantic_and_station_complete() {
    let corridor = typical_corridor();
    let sections = corridor.cross_section_profile().sections();
    assert_eq!(
        sections[0]
            .layout()
            .left()
            .iter()
            .map(|lane| lane.lane_id())
            .collect::<Vec<_>>(),
        vec![lane_id(2), lane_id(1)]
    );
    assert_eq!(
        sections[0]
            .layout()
            .right()
            .iter()
            .map(|lane| lane.lane_id())
            .collect::<Vec<_>>(),
        vec![lane_id(3), lane_id(4)]
    );
    assert_eq!(
        corridor.lane(lane_id(2)).unwrap().direction(),
        LaneDirection::AgainstReference
    );
    assert_eq!(
        corridor.lane(lane_id(3)).unwrap().direction(),
        LaneDirection::AlongReference
    );
    let ranges = corridor.section_ranges();
    assert_eq!(
        (ranges[0].start().get(), ranges[0].end().get()),
        (0.0, 60.0)
    );
    assert_eq!(
        (ranges[1].start().get(), ranges[1].end().get()),
        (60.0, 100.0)
    );
    assert_eq!(sections[0].layout().total_width().get(), 13.5);
}

#[test]
fn invalid_width_layout_and_profile_boundaries_are_rejected() {
    assert_eq!(
        LaneSlice::new(lane_id(1), length(0.0)).unwrap_err().code(),
        CorridorErrorCode::ZeroLaneWidth
    );
    assert_eq!(
        CrossSectionLayout::new(vec![], vec![]).unwrap_err().code(),
        CorridorErrorCode::EmptyLaneLayout
    );
    assert_eq!(
        CrossSectionLayout::new(vec![slice(1, 3.0)], vec![slice(1, 3.0)])
            .unwrap_err()
            .code(),
        CorridorErrorCode::DuplicateLaneReference
    );
    assert_eq!(
        CrossSectionProfile::new(vec![]).unwrap_err().code(),
        CorridorErrorCode::EmptyCrossSectionProfile
    );
    let layout = || CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap();
    assert_eq!(
        CrossSectionProfile::new(vec![CrossSectionSection::new(length(1.0), layout())])
            .unwrap_err()
            .code(),
        CorridorErrorCode::FirstSectionNotAtZero
    );
    assert_eq!(
        CrossSectionProfile::new(vec![
            CrossSectionSection::new(length(0.0), layout()),
            CrossSectionSection::new(length(0.0), layout())
        ])
        .unwrap_err()
        .code(),
        CorridorErrorCode::SectionStationNotIncreasing
    );
}

#[test]
fn catalog_and_lane_references_are_validated_with_object_evidence() {
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0),
        CrossSectionLayout::new(vec![], vec![slice(99, 3.0)]).unwrap(),
    )])
    .unwrap();
    let error = Corridor::new(
        CorridorId::from_u128(5),
        reference_line(10.0),
        vec![definition(1, LaneDirection::AlongReference)],
        profile,
    )
    .unwrap_err();
    assert_eq!(error.code(), CorridorErrorCode::DanglingLaneReference);
    assert_eq!(error.section_index(), Some(0));
    assert_eq!(error.lane_id(), Some(lane_id(99)));

    let corridor = typical_corridor();
    let duplicate = DesignCatalog::new(vec![corridor.clone(), corridor]).unwrap_err();
    assert_eq!(duplicate.code(), CorridorErrorCode::DuplicateCorridor);
    assert_eq!(duplicate.corridor_id(), Some(CorridorId::from_u128(10)));

    let first = typical_corridor_with_id(10);
    let second = typical_corridor_with_id(11);
    let duplicate_lane = DesignCatalog::new(vec![first.clone(), second.clone()]).unwrap_err();
    assert_eq!(
        duplicate_lane.code(),
        CorridorErrorCode::DuplicateProjectLaneId
    );
    assert_eq!(
        duplicate_lane.corridor_id(),
        Some(CorridorId::from_u128(11))
    );
    assert_eq!(duplicate_lane.lane_id(), Some(lane_id(1)));

    let malformed = serde_json::json!({"corridors": [first, second]});
    assert!(serde_json::from_value::<DesignCatalog>(malformed).is_err());
}

#[test]
fn catalog_profile_and_width_aggregate_failures_are_explicit() {
    let one_lane_profile = || {
        CrossSectionProfile::new(vec![CrossSectionSection::new(
            length(0.0),
            CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap(),
        )])
        .unwrap()
    };
    let duplicate = Corridor::new(
        CorridorId::from_u128(1),
        reference_line(10.0),
        vec![
            definition(1, LaneDirection::AlongReference),
            definition(1, LaneDirection::AgainstReference),
        ],
        one_lane_profile(),
    )
    .unwrap_err();
    assert_eq!(duplicate.code(), CorridorErrorCode::DuplicateLaneDefinition);
    assert_eq!(duplicate.lane_id(), Some(lane_id(1)));

    let unused = Corridor::new(
        CorridorId::from_u128(1),
        reference_line(10.0),
        vec![
            definition(1, LaneDirection::AlongReference),
            definition(2, LaneDirection::AlongReference),
        ],
        one_lane_profile(),
    )
    .unwrap_err();
    assert_eq!(unused.code(), CorridorErrorCode::UnusedLaneDefinition);
    assert_eq!(unused.lane_id(), Some(lane_id(2)));

    let outside = CrossSectionProfile::new(vec![
        CrossSectionSection::new(
            length(0.0),
            CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap(),
        ),
        CrossSectionSection::new(
            length(10.0),
            CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap(),
        ),
    ])
    .unwrap();
    let error = Corridor::new(
        CorridorId::from_u128(1),
        reference_line(10.0),
        vec![definition(1, LaneDirection::AlongReference)],
        outside,
    )
    .unwrap_err();
    assert_eq!(error.code(), CorridorErrorCode::SectionOutsideReferenceLine);
    assert_eq!(error.section_index(), Some(1));

    let huge = LaneSlice::new(lane_id(1), length(f64::MAX)).unwrap();
    let overflow = CrossSectionLayout::new(
        vec![huge],
        vec![LaneSlice::new(lane_id(2), length(f64::MAX)).unwrap()],
    )
    .unwrap_err();
    assert_eq!(
        overflow.code(),
        CorridorErrorCode::CrossSectionWidthOverflow
    );
    let resolution_loss = CrossSectionLayout::new(vec![huge], vec![slice(2, 1.0)]).unwrap_err();
    assert_eq!(
        resolution_loss.code(),
        CorridorErrorCode::CrossSectionWidthResolutionLost
    );
}

#[test]
fn cross_field_invariants_are_rechecked_during_deserialization() {
    let corridor = typical_corridor();
    let mut value = serde_json::to_value(&corridor).unwrap();
    value["cross_section_profile"][1]["start_station"] = serde_json::json!(100.0);
    let error = serde_json::from_value::<Corridor>(value).unwrap_err();
    assert!(error.to_string().contains("section_outside_reference_line"));

    let mut value = serde_json::to_value(&corridor).unwrap();
    value["cross_section_profile"][0]["layout"]["right"][0]["lane_id"] =
        serde_json::to_value(lane_id(999)).unwrap();
    assert!(serde_json::from_value::<Corridor>(value).is_err());

    let mut value = serde_json::to_value(&corridor).unwrap();
    value["cross_section_profile"][0]["layout"]["left"][0]["width_m"] = serde_json::json!(0.0);
    assert!(serde_json::from_value::<Corridor>(value).is_err());

    let json = serde_json::to_string(&corridor).unwrap();
    assert_eq!(serde_json::from_str::<Corridor>(&json).unwrap(), corridor);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn monotonic_section_starts_derive_contiguous_full_coverage(
        mut starts in prop::collection::vec(1_u16..999, 0..24)
    ) {
        starts.sort_unstable();
        starts.dedup();
        let mut sections = vec![CrossSectionSection::new(length(0.0),
            CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap())];
        sections.extend(starts.iter().map(|value| CrossSectionSection::new(length(f64::from(*value)),
            CrossSectionLayout::new(vec![], vec![slice(1, 3.0)]).unwrap())));
        let corridor = Corridor::new(CorridorId::from_u128(1), reference_line(1000.0),
            vec![definition(1, LaneDirection::AlongReference)], CrossSectionProfile::new(sections).unwrap()).unwrap();
        let ranges = corridor.section_ranges();
        prop_assert_eq!(ranges[0].start().get(), 0.0);
        for pair in ranges.windows(2) { prop_assert_eq!(pair[0].end(), pair[1].start()); }
        prop_assert_eq!(ranges.last().unwrap().end().get(), 1000.0);
    }
}
