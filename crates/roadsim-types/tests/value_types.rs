use roadsim_types::{
    AngleRadians, CoordinateMeters, CorridorId, DurationSeconds, LaneId, LengthMeters, ObjectKind,
    ObjectRef, ProjectId, SimulationTick, SpeedMetersPerSecond, ToleranceMeters, ValueErrorCode,
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
        ToleranceMeters::try_new(-0.1).unwrap_err().code(),
        ValueErrorCode::Negative
    );
    assert_eq!(LengthMeters::try_new(0.0).unwrap().get(), 0.0);
    assert_eq!(ToleranceMeters::try_new(0.0).unwrap().get(), 0.0);
    assert!(
        CoordinateMeters::try_new(-0.0)
            .unwrap()
            .get()
            .is_sign_positive()
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
