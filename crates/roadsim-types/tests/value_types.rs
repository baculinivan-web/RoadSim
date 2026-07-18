use roadsim_types::{
    AngleRadians, CoordinateMeters, CorridorId, CurvaturePerMeter, CurvatureTolerancePerMeter,
    DurationSeconds, FlowRatePerHour, HeadingRadians, LaneId, LengthMeters, ObjectKind, ObjectRef,
    ProjectId, RailAlignmentId, RootSeed, Sha256Digest, SidewalkId, SimulationTick,
    SpeedMetersPerSecond, ToleranceMeters, ValueErrorCode, WalkingAreaId,
};

#[test]
fn typed_ids_round_trip_as_stable_uuid_strings() {
    let id = ProjectId::from_u128(0x67e5_5044_10b1_426f_9247_bb68_06c5_8c8a);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"67e55044-10b1-426f-9247-bb6806c58c8a\"");
    assert_eq!(serde_json::from_str::<ProjectId>(&json).unwrap(), id);
    assert_eq!(id.to_string().parse::<ProjectId>().unwrap(), id);
    assert!("not-a-uuid".parse::<ProjectId>().is_err());
}

#[test]
fn object_refs_preserve_kind_and_uuid() {
    let corridor = CorridorId::from_u128(42);
    let object_ref = ObjectRef::from(corridor);
    assert_eq!(object_ref.kind(), ObjectKind::Corridor);
    assert_eq!(object_ref.uuid(), corridor.as_uuid());
    assert_ne!(ObjectRef::from(LaneId::from_u128(42)), object_ref);
    assert_eq!(
        ObjectRef::from(WalkingAreaId::from_u128(42)).kind(),
        ObjectKind::WalkingArea
    );
    assert_eq!(
        ObjectRef::from(SidewalkId::from_u128(42)).kind(),
        ObjectKind::Sidewalk
    );
    assert_eq!(
        ObjectRef::from(RailAlignmentId::from_u128(42)).kind(),
        ObjectKind::RailAlignment
    );
}

#[test]
fn scalar_types_reject_non_finite_and_domain_invalid_values() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            CoordinateMeters::try_new(value).unwrap_err().code(),
            ValueErrorCode::NonFinite
        );
        assert_eq!(
            AngleRadians::try_new(value).unwrap_err().code(),
            ValueErrorCode::NonFinite
        );
        assert_eq!(
            CurvaturePerMeter::try_new(value).unwrap_err().code(),
            ValueErrorCode::NonFinite
        );
        assert_eq!(
            HeadingRadians::try_new(value).unwrap_err().code(),
            ValueErrorCode::NonFinite
        );
    }

    assert_eq!(
        LengthMeters::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(
        SpeedMetersPerSecond::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(
        DurationSeconds::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(
        FlowRatePerHour::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(
        ToleranceMeters::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(LengthMeters::try_new(0.0).unwrap().get(), 0.0);
    assert_eq!(ToleranceMeters::try_new(0.0).unwrap().get(), 0.0);
    assert_eq!(CurvaturePerMeter::try_new(-0.1).unwrap().get(), -0.1);
    assert_eq!(
        CurvatureTolerancePerMeter::try_new(-0.1)
            .unwrap_err()
            .code(),
        ValueErrorCode::Negative
    );
    assert!(
        CoordinateMeters::try_new(-0.0)
            .unwrap()
            .get()
            .is_sign_positive()
    );
}

#[test]
fn headings_have_one_canonical_full_turn_representation() {
    let tau = std::f64::consts::TAU;
    assert_eq!(HeadingRadians::try_new(0.0).unwrap().get(), 0.0);
    assert_eq!(HeadingRadians::try_new(-0.0).unwrap().get(), 0.0);
    assert_eq!(HeadingRadians::try_new(tau).unwrap().get(), 0.0);
    assert_eq!(HeadingRadians::try_new(-tau).unwrap().get(), 0.0);
    for tiny_negative in [-f64::EPSILON, -f64::MIN_POSITIVE, -f64::from_bits(1)] {
        let heading = HeadingRadians::try_new(tiny_negative).unwrap();
        assert!(heading.get() >= 0.0);
        assert!(heading.get() < tau);
        let json = serde_json::to_string(&heading).unwrap();
        assert_eq!(
            serde_json::from_str::<HeadingRadians>(&json).unwrap(),
            heading
        );
    }
    assert_eq!(
        HeadingRadians::try_new(-std::f64::consts::FRAC_PI_2)
            .unwrap()
            .get(),
        3.0 * std::f64::consts::FRAC_PI_2
    );
    assert_eq!(
        serde_json::from_str::<HeadingRadians>(&tau.to_string())
            .unwrap()
            .get(),
        0.0
    );
}

#[test]
fn serialized_units_are_locale_independent_numbers() {
    let length = LengthMeters::try_new(12.5).unwrap();
    assert_eq!(serde_json::to_string(&length).unwrap(), "12.5");
    assert_eq!(
        serde_json::to_string(&CoordinateMeters::try_new(-0.0).unwrap()).unwrap(),
        "0.0"
    );
    assert_eq!(
        serde_json::from_str::<LengthMeters>("12.5").unwrap(),
        length
    );
    assert!(serde_json::from_str::<LengthMeters>("1e400").is_err());
    assert!(serde_json::from_str::<LengthMeters>("-1.0").is_err());
}

#[test]
fn simulation_ticks_are_integer_and_checked() {
    let tick = SimulationTick::new(41);
    assert_eq!(serde_json::to_string(&tick).unwrap(), "41");
    assert_eq!(tick.checked_add(1), Some(SimulationTick::new(42)));
    assert_eq!(SimulationTick::new(u64::MAX).checked_add(1), None);
}

#[test]
fn root_seed_is_explicit_and_round_trips_without_a_default() {
    let seed = RootSeed::new(u64::MAX);
    let encoded = serde_json::to_string(&seed).unwrap();
    assert_eq!(encoded, u64::MAX.to_string());
    assert_eq!(serde_json::from_str::<RootSeed>(&encoded).unwrap(), seed);
}

#[test]
fn sha256_digest_has_one_canonical_wire_encoding() {
    let digest = Sha256Digest::from_bytes([0xab; 32]);
    let encoded = serde_json::to_string(&digest).unwrap();
    assert_eq!(encoded, format!("\"{}\"", "ab".repeat(32)));
    assert_eq!(
        serde_json::from_str::<Sha256Digest>(&encoded).unwrap(),
        digest
    );
    assert!(serde_json::from_str::<Sha256Digest>(&format!("\"{}\"", "AB".repeat(32))).is_err());
    assert!(serde_json::from_str::<Sha256Digest>("\"1234\"").is_err());
}
