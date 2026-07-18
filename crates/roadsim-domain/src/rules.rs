use crate::multimodal::duplicate_object_ref;
use roadsim_types::{ObjectRef, RuleExceptionId, Sha256Digest};
use serde::{Deserialize, Deserializer, Serialize};
use std::{error::Error, fmt};

const MAX_RULE_ID_BYTES: usize = 256;
const MAX_RULESET_FIELD_BYTES: usize = 128;
const MAX_ACTOR_BYTES: usize = 256;
const MAX_EXCEPTION_TEXT_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleConfigErrorCode {
    EmptyText,
    TextTooLong,
    MissingRulesetPin,
    DuplicateException,
}

impl RuleConfigErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyText => "domain.rules.text.empty",
            Self::TextTooLong => "domain.rules.text.too_long",
            Self::MissingRulesetPin => "domain.rules.pin.missing",
            Self::DuplicateException => "domain.rules.exception.duplicate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleConfigError {
    code: RuleConfigErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl RuleConfigError {
    fn new(code: RuleConfigErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> RuleConfigErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for RuleConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for RuleConfigError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct RuleText<const MAX: usize>(String);

impl<const MAX: usize> RuleText<MAX> {
    fn new(value: impl Into<String>) -> Result<Self, RuleConfigError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RuleConfigError::new(RuleConfigErrorCode::EmptyText, vec![]));
        }
        if value.len() > MAX {
            return Err(RuleConfigError::new(
                RuleConfigErrorCode::TextTooLong,
                vec![],
            ));
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for RuleText<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Exact immutable ruleset selection. Loading or evaluating a different
/// artifact requires an explicit project change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RulesetPin {
    ruleset_id: RuleText<MAX_RULESET_FIELD_BYTES>,
    exact_version: RuleText<MAX_RULESET_FIELD_BYTES>,
    artifact_sha256: Sha256Digest,
}

impl RulesetPin {
    pub fn new(
        ruleset_id: impl Into<String>,
        exact_version: impl Into<String>,
        artifact_sha256: Sha256Digest,
    ) -> Result<Self, RuleConfigError> {
        Ok(Self {
            ruleset_id: RuleText::new(ruleset_id)?,
            exact_version: RuleText::new(exact_version)?,
            artifact_sha256,
        })
    }

    #[must_use]
    pub fn ruleset_id(&self) -> &str {
        self.ruleset_id.as_str()
    }
    #[must_use]
    pub fn exact_version(&self) -> &str {
        self.exact_version.as_str()
    }
    #[must_use]
    pub const fn artifact_sha256(&self) -> Sha256Digest {
        self.artifact_sha256
    }
}

impl<'de> Deserialize<'de> for RulesetPin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            ruleset_id: String,
            exact_version: String,
            artifact_sha256: Sha256Digest,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.ruleset_id, wire.exact_version, wire.artifact_sha256)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExceptionStatus {
    Accepted,
    Stale,
}

/// Audited acceptance of one rule finding for one object state.
///
/// This is configuration only; it does not contain or interpret a normative
/// requirement. A changed object hash or ruleset artifact makes it stale.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleException {
    id: RuleExceptionId,
    rule_id: RuleText<MAX_RULE_ID_BYTES>,
    object_ref: ObjectRef,
    accepted_object_hash: Sha256Digest,
    accepted_ruleset_hash: Sha256Digest,
    accepted_by: RuleText<MAX_ACTOR_BYTES>,
    accepted_at_unix_s: u64,
    basis: RuleText<MAX_EXCEPTION_TEXT_BYTES>,
    comment: RuleText<MAX_EXCEPTION_TEXT_BYTES>,
}

impl RuleException {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RuleExceptionId,
        rule_id: impl Into<String>,
        object_ref: ObjectRef,
        accepted_object_hash: Sha256Digest,
        accepted_ruleset_hash: Sha256Digest,
        accepted_by: impl Into<String>,
        accepted_at_unix_s: u64,
        basis: impl Into<String>,
        comment: impl Into<String>,
    ) -> Result<Self, RuleConfigError> {
        Ok(Self {
            id,
            rule_id: RuleText::new(rule_id)?,
            object_ref,
            accepted_object_hash,
            accepted_ruleset_hash,
            accepted_by: RuleText::new(accepted_by)?,
            accepted_at_unix_s,
            basis: RuleText::new(basis)?,
            comment: RuleText::new(comment)?,
        })
    }

    #[must_use]
    pub const fn id(&self) -> RuleExceptionId {
        self.id
    }
    #[must_use]
    pub fn rule_id(&self) -> &str {
        self.rule_id.as_str()
    }
    #[must_use]
    pub const fn object_ref(&self) -> ObjectRef {
        self.object_ref
    }
    #[must_use]
    pub const fn accepted_object_hash(&self) -> Sha256Digest {
        self.accepted_object_hash
    }
    #[must_use]
    pub const fn accepted_ruleset_hash(&self) -> Sha256Digest {
        self.accepted_ruleset_hash
    }
    #[must_use]
    pub fn accepted_by(&self) -> &str {
        self.accepted_by.as_str()
    }
    #[must_use]
    pub const fn accepted_at_unix_s(&self) -> u64 {
        self.accepted_at_unix_s
    }
    #[must_use]
    pub fn basis(&self) -> &str {
        self.basis.as_str()
    }
    #[must_use]
    pub fn comment(&self) -> &str {
        self.comment.as_str()
    }

    #[must_use]
    pub fn status(
        &self,
        current_object_hash: Sha256Digest,
        current_ruleset_hash: Sha256Digest,
    ) -> ExceptionStatus {
        if self.accepted_object_hash == current_object_hash
            && self.accepted_ruleset_hash == current_ruleset_hash
        {
            ExceptionStatus::Accepted
        } else {
            ExceptionStatus::Stale
        }
    }
}

impl<'de> Deserialize<'de> for RuleException {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: RuleExceptionId,
            rule_id: String,
            object_ref: ObjectRef,
            accepted_object_hash: Sha256Digest,
            accepted_ruleset_hash: Sha256Digest,
            accepted_by: String,
            accepted_at_unix_s: u64,
            basis: String,
            comment: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.rule_id,
            wire.object_ref,
            wire.accepted_object_hash,
            wire.accepted_ruleset_hash,
            wire.accepted_by,
            wire.accepted_at_unix_s,
            wire.basis,
            wire.comment,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RuleConfiguration {
    ruleset_pin: Option<RulesetPin>,
    exceptions: Vec<RuleException>,
}

impl RuleConfiguration {
    #[must_use]
    pub const fn unconfigured() -> Self {
        Self {
            ruleset_pin: None,
            exceptions: Vec::new(),
        }
    }

    pub fn new(
        ruleset_pin: Option<RulesetPin>,
        mut exceptions: Vec<RuleException>,
    ) -> Result<Self, RuleConfigError> {
        if ruleset_pin.is_none() && !exceptions.is_empty() {
            return Err(RuleConfigError::new(
                RuleConfigErrorCode::MissingRulesetPin,
                exceptions.iter().map(|item| item.id().into()).collect(),
            ));
        }
        exceptions.sort_by_key(RuleException::id);
        if let Some(duplicate) = duplicate_object_ref(exceptions.iter().map(RuleException::id)) {
            return Err(RuleConfigError::new(
                RuleConfigErrorCode::DuplicateException,
                vec![duplicate],
            ));
        }
        Ok(Self {
            ruleset_pin,
            exceptions,
        })
    }

    #[must_use]
    pub const fn ruleset_pin(&self) -> Option<&RulesetPin> {
        self.ruleset_pin.as_ref()
    }
    #[must_use]
    pub fn exceptions(&self) -> &[RuleException] {
        &self.exceptions
    }
}

impl<'de> Deserialize<'de> for RuleConfiguration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            ruleset_pin: Option<RulesetPin>,
            exceptions: Vec<RuleException>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.ruleset_pin, wire.exceptions).map_err(serde::de::Error::custom)
    }
}
