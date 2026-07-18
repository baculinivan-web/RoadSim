use proptest::prelude::*;
use roadsim_domain::{
    CatalogErrorCode, Corridor, CorridorEnd, CorridorEndpointRef, CorridorSide, CrossSectionLayout,
    CrossSectionProfile, CrossSectionSection, Crossing, DesignCatalog, Junction, LaneDefinition,
    LaneDirection, LaneSlice, LaneUse, MultimodalErrorCode, Point2Meters, RailAlignment,
    ReferenceLine, ReferenceLineElement, ReferenceLinePose, Sidewalk, WalkingArea,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, CrossingId, HeadingRadians, JunctionId, LaneId, LengthMeters,
    RailAlignmentId, SidewalkId, WalkingAreaId,
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
fn line(x: f64, length_m: f64) -> ReferenceLine {
    ReferenceLine::new(
        ReferenceLinePose::new(point(x, 0.0), HeadingRadians::try_new(0.0).unwrap()),
        vec![ReferenceLineElement::line(length(length_m)).unwrap()],
    )
    .unwrap()
}
fn corridor(id: u128, lane_id: u128, x: f64) -> Corridor {
    let lane_id = LaneId::from_u128(lane_id);
    Corridor::new(
        CorridorId::from_u128(id),
        line(x, 50.0),
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
fn area(id: u128, x: f64) -> WalkingArea {
    WalkingArea::new(
        WalkingAreaId::from_u128(id),
        vec![point(x, 0.0), point(x + 2.0, 0.0), point(x + 1.0, 2.0)],
    )
    .unwrap()
}

#[test]
fn typical_multimodal_catalog_is_backend_independent_and_deterministic() {
    let first = corridor(10, 20, 0.0);
    let second = corridor(11, 21, 50.0);
    let left = area(30, 10.0);
    let right = area(31, 20.0);
    let catalog = DesignCatalog::with_multimodal(
        vec![second, first],
        vec![
            Junction::new(
                JunctionId::from_u128(40),
                vec![
                    CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
                    CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
                ],
            )
            .unwrap(),
        ],
        vec![right, left],
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
        vec![
            RailAlignment::new(
                RailAlignmentId::from_u128(70),
                line(0.0, 100.0),
                length(1.435),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    assert_eq!(catalog.corridors()[0].id(), CorridorId::from_u128(10));
    assert_eq!(
        catalog.walking_areas()[0].id(),
        WalkingAreaId::from_u128(30)
    );
    assert_eq!(catalog.junctions().len(), 1);
    assert_eq!(catalog.sidewalks().len(), 1);
    assert_eq!(catalog.crossings().len(), 1);
    assert_eq!(catalog.rail_alignments()[0].track_gauge().get(), 1.435);

    let round_trip: DesignCatalog =
        serde_json::from_str(&serde_json::to_string(&catalog).unwrap()).unwrap();
    assert_eq!(round_trip, catalog);
}

#[test]
fn dangling_typed_references_have_stable_codes_and_object_evidence() {
    let junction = Junction::new(
        JunctionId::from_u128(40),
        vec![
            CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End),
            CorridorEndpointRef::new(CorridorId::from_u128(999), CorridorEnd::Start),
        ],
    )
    .unwrap();
    let error = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![junction],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::DanglingCorridor);
    assert_eq!(error.object_refs().len(), 2);

    let crossing = Crossing::new(
        CrossingId::from_u128(60),
        CorridorId::from_u128(10),
        length(25.0),
        length(4.0),
        WalkingAreaId::from_u128(30),
        WalkingAreaId::from_u128(999),
    )
    .unwrap();
    let error = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![],
        vec![area(30, 0.0)],
        vec![],
        vec![crossing],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::DanglingWalkingArea);
    assert_eq!(error.object_refs().len(), 2);
}

#[test]
fn aggregate_rejects_duplicate_endpoint_and_out_of_range_station() {
    let endpoint = CorridorEndpointRef::new(CorridorId::from_u128(10), CorridorEnd::End);
    let first = Junction::new(
        JunctionId::from_u128(40),
        vec![
            endpoint,
            CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::Start),
        ],
    )
    .unwrap();
    let second = Junction::new(
        JunctionId::from_u128(41),
        vec![
            endpoint,
            CorridorEndpointRef::new(CorridorId::from_u128(11), CorridorEnd::End),
        ],
    )
    .unwrap();
    let error = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0), corridor(11, 21, 50.0)],
        vec![first, second],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::EndpointAlreadyConnected);
    assert_eq!(error.object_refs().len(), 3);

    let sidewalk = Sidewalk::new(
        SidewalkId::from_u128(50),
        CorridorId::from_u128(10),
        CorridorSide::Left,
        length(0.0),
        length(51.0),
        length(2.0),
    )
    .unwrap();
    let error = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![],
        vec![],
        vec![sidewalk],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), CatalogErrorCode::StationOutsideCorridor);
}

#[test]
fn local_invariants_are_rechecked_during_deserialization() {
    let junction_error = Junction::new(
        JunctionId::from_u128(1),
        vec![CorridorEndpointRef::new(
            CorridorId::from_u128(2),
            CorridorEnd::Start,
        )],
    )
    .unwrap_err();
    assert_eq!(
        junction_error.code(),
        MultimodalErrorCode::TooFewJunctionApproaches
    );
    assert_eq!(
        junction_error.object_refs(),
        &[JunctionId::from_u128(1).into()]
    );
    assert_eq!(
        WalkingArea::new(
            WalkingAreaId::from_u128(1),
            vec![point(0.0, 0.0), point(1.0, 0.0)]
        )
        .unwrap_err()
        .code(),
        MultimodalErrorCode::TooFewWalkingAreaVertices
    );
    let invalid = r#"{"id":"00000000-0000-0000-0000-000000000001","corridor_id":"00000000-0000-0000-0000-000000000002","side":"left","start_station_m":10.0,"end_station_m":5.0,"width_m":2.0}"#;
    assert!(serde_json::from_str::<Sidewalk>(invalid).is_err());

    let crossing_error = Crossing::new(
        CrossingId::from_u128(60),
        CorridorId::from_u128(10),
        length(10.0),
        length(0.0),
        WalkingAreaId::from_u128(30),
        WalkingAreaId::from_u128(31),
    )
    .unwrap_err();
    assert_eq!(crossing_error.code(), MultimodalErrorCode::ZeroWidth);
    assert_eq!(crossing_error.object_refs().len(), 2);
}

#[test]
fn aggregate_invariants_are_rechecked_during_deserialization() {
    let crossing = Crossing::new(
        CrossingId::from_u128(60),
        CorridorId::from_u128(10),
        length(25.0),
        length(4.0),
        WalkingAreaId::from_u128(30),
        WalkingAreaId::from_u128(31),
    )
    .unwrap();
    let catalog = DesignCatalog::with_multimodal(
        vec![corridor(10, 20, 0.0)],
        vec![],
        vec![area(30, 0.0), area(31, 4.0)],
        vec![],
        vec![crossing],
        vec![],
    )
    .unwrap();
    let mut wire = serde_json::to_value(catalog).unwrap();
    wire["crossings"][0]["corridor_id"] = serde_json::json!(CorridorId::from_u128(999));

    let error = serde_json::from_value::<DesignCatalog>(wire).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("domain.catalog.corridor_ref.dangling")
    );
}

proptest! {
    #[test]
    fn valid_station_attachments_remain_inside_their_corridor(
        sidewalk_end in 0.01_f64..=50.0,
        crossing_station in 0.0_f64..50.0,
    ) {
        let catalog = DesignCatalog::with_multimodal(
            vec![corridor(10, 20, 0.0)],
            vec![],
            vec![area(30, 0.0), area(31, 4.0)],
            vec![Sidewalk::new(
                SidewalkId::from_u128(50),
                CorridorId::from_u128(10),
                CorridorSide::Right,
                length(0.0),
                length(sidewalk_end),
                length(2.0),
            ).unwrap()],
            vec![Crossing::new(
                CrossingId::from_u128(60),
                CorridorId::from_u128(10),
                length(crossing_station),
                length(4.0),
                WalkingAreaId::from_u128(30),
                WalkingAreaId::from_u128(31),
            ).unwrap()],
            vec![],
        ).unwrap();

        prop_assert_eq!(catalog.sidewalks()[0].end_station().get(), sidewalk_end);
        prop_assert_eq!(catalog.crossings()[0].station().get(), crossing_station);
    }
}
