use proptest::prelude::*;
use roadsim_domain::{
    ExceptionStatus, RuleConfigErrorCode, RuleConfiguration, RuleException, RulesetPin,
};
use roadsim_types::{CorridorId, RuleExceptionId, Sha256Digest};

fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}

fn exception(id: u128) -> RuleException {
    RuleException::new(
        RuleExceptionId::from_u128(id),
        "RU.example.reviewed-rule",
        CorridorId::from_u128(10).into(),
        digest(1),
        digest(2),
        "reviewer@example.org",
        1_753_987_200,
        "Site-specific engineering decision approved by the owner",
        "Re-evaluate after any corridor or ruleset change",
    )
    .unwrap()
}

#[test]
fn exception_becomes_stale_on_object_or_ruleset_hash_change() {
    let exception = exception(20);
    assert_eq!(
        exception.status(digest(1), digest(2)),
        ExceptionStatus::Accepted
    );
    assert_eq!(
        exception.status(digest(9), digest(2)),
        ExceptionStatus::Stale
    );
    assert_eq!(
        exception.status(digest(1), digest(9)),
        ExceptionStatus::Stale
    );
}

#[test]
fn exact_ruleset_pin_and_audit_fields_round_trip() {
    let pin = RulesetPin::new("RU-baseline", "RU-2026.07.0", digest(2)).unwrap();
    let config = RuleConfiguration::new(Some(pin), vec![exception(20)]).unwrap();

    assert_eq!(config.ruleset_pin().unwrap().ruleset_id(), "RU-baseline");
    assert_eq!(
        config.ruleset_pin().unwrap().exact_version(),
        "RU-2026.07.0"
    );
    assert_eq!(config.exceptions()[0].accepted_at_unix_s(), 1_753_987_200);
    assert_eq!(config.exceptions()[0].accepted_by(), "reviewer@example.org");

    let encoded = serde_json::to_string(&config).unwrap();
    let decoded: RuleConfiguration = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, config);
}

#[test]
fn exceptions_require_a_pin_and_stable_unique_ids() {
    let error = RuleConfiguration::new(None, vec![exception(20)]).unwrap_err();
    assert_eq!(error.code(), RuleConfigErrorCode::MissingRulesetPin);
    assert_eq!(
        error.object_refs(),
        &[RuleExceptionId::from_u128(20).into()]
    );

    let pin = RulesetPin::new("RU-baseline", "RU-2026.07.0", digest(2)).unwrap();
    let error = RuleConfiguration::new(Some(pin), vec![exception(20), exception(20)]).unwrap_err();
    assert_eq!(error.code(), RuleConfigErrorCode::DuplicateException);
    assert_eq!(
        error.object_refs(),
        &[RuleExceptionId::from_u128(20).into()]
    );
}

#[test]
fn deserialization_rechecks_bounded_audit_text() {
    let pin = RulesetPin::new("RU-baseline", "RU-2026.07.0", digest(2)).unwrap();
    let config = RuleConfiguration::new(Some(pin), vec![exception(20)]).unwrap();
    let mut value = serde_json::to_value(config).unwrap();
    value["exceptions"][0]["basis"] = serde_json::json!("");
    assert!(serde_json::from_value::<RuleConfiguration>(value).is_err());
}

proptest! {
    #[test]
    fn accepted_status_requires_both_exact_hashes(
        object_bytes in any::<[u8; 32]>(),
        ruleset_bytes in any::<[u8; 32]>(),
    ) {
        let status = exception(20).status(
            Sha256Digest::from_bytes(object_bytes),
            Sha256Digest::from_bytes(ruleset_bytes),
        );
        let expected = if object_bytes == [1; 32] && ruleset_bytes == [2; 32] {
            ExceptionStatus::Accepted
        } else {
            ExceptionStatus::Stale
        };
        prop_assert_eq!(status, expected);
    }
}
