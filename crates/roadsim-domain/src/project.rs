use crate::{DesignCatalog, RuleConfiguration, ScenarioError, StudyCatalog};
use roadsim_types::{CoordinateMeters, ProjectId};
use serde::{Deserialize, Deserializer, Serialize};
use std::{error::Error, fmt};

const MAX_PROJECT_NAME_BYTES: usize = 256;
const MAX_AUTHORITY_BYTES: usize = 64;
const MAX_CRS_CODE_BYTES: usize = 128;
const MAX_WKT_BYTES: usize = 65_536;
const MAX_PROVENANCE_FIELD_BYTES: usize = 4_096;
const MAX_VERTICAL_DATUM_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainErrorCode {
    EmptyText,
    TextTooLong,
    NonMetricEngineeringCrs,
}

impl DomainErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyText => "domain.text.empty",
            Self::TextTooLong => "domain.text.too_long",
            Self::NonMetricEngineeringCrs => "domain.crs.engineering_not_metric",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainError {
    code: DomainErrorCode,
    field: &'static str,
}

impl DomainError {
    #[must_use]
    pub const fn code(self) -> DomainErrorCode {
        self.code
    }

    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} for {}", self.code.as_str(), self.field)
    }
}

impl Error for DomainError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct BoundedText<const MAX: usize>(String);

impl<const MAX: usize> BoundedText<MAX> {
    fn try_new(value: impl Into<String>, field: &'static str) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError {
                code: DomainErrorCode::EmptyText,
                field,
            });
        }
        if value.len() > MAX {
            return Err(DomainError {
                code: DomainErrorCode::TextTooLong,
                field,
            });
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for BoundedText<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value, "serialized_text").map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectMetadata {
    name: BoundedText<MAX_PROJECT_NAME_BYTES>,
}

impl ProjectMetadata {
    pub fn new(name: impl Into<String>) -> Result<Self, DomainError> {
        Ok(Self {
            name: BoundedText::try_new(name, "project.name")?,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthorityCrs {
    authority: BoundedText<MAX_AUTHORITY_BYTES>,
    code: BoundedText<MAX_CRS_CODE_BYTES>,
}

impl AuthorityCrs {
    pub fn new(authority: impl Into<String>, code: impl Into<String>) -> Result<Self, DomainError> {
        Ok(Self {
            authority: BoundedText::try_new(authority, "crs.authority")?,
            code: BoundedText::try_new(code, "crs.code")?,
        })
    }

    #[must_use]
    pub fn authority(&self) -> &str {
        self.authority.as_str()
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.code.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WktCrs {
    wkt: BoundedText<MAX_WKT_BYTES>,
}

impl WktCrs {
    pub fn new(wkt: impl Into<String>) -> Result<Self, DomainError> {
        Ok(Self {
            wkt: BoundedText::try_new(wkt, "crs.wkt")?,
        })
    }

    #[must_use]
    pub fn wkt(&self) -> &str {
        self.wkt.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "definition")]
pub enum CrsDefinition {
    Authority(AuthorityCrs),
    Wkt(WktCrs),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineeringUnit {
    Metre,
    Degree,
    Foot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EngineeringCrsDescriptor {
    definition: CrsDefinition,
    declared_unit: EngineeringUnit,
}

#[derive(Deserialize)]
struct EngineeringCrsWire {
    definition: CrsDefinition,
    declared_unit: EngineeringUnit,
}

impl<'de> Deserialize<'de> for EngineeringCrsDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = EngineeringCrsWire::deserialize(deserializer)?;
        Self::new(value.definition, value.declared_unit).map_err(serde::de::Error::custom)
    }
}

impl EngineeringCrsDescriptor {
    /// Creates a descriptor whose declared engineering unit is metric.
    ///
    /// Consistency between an authority/WKT definition and the declaration is
    /// verified by the isolated PROJ coordinate service in `E14-T06`.
    pub fn new(
        definition: CrsDefinition,
        declared_unit: EngineeringUnit,
    ) -> Result<Self, DomainError> {
        if declared_unit != EngineeringUnit::Metre {
            return Err(DomainError {
                code: DomainErrorCode::NonMetricEngineeringCrs,
                field: "crs.engineering.declared_unit",
            });
        }
        Ok(Self {
            definition,
            declared_unit,
        })
    }

    #[must_use]
    pub const fn declared_unit(&self) -> EngineeringUnit {
        self.declared_unit
    }

    #[must_use]
    pub const fn definition(&self) -> &CrsDefinition {
        &self.definition
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisOrder {
    EastNorth,
    NorthEast,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalOrigin {
    east_m: CoordinateMeters,
    north_m: CoordinateMeters,
}

impl LocalOrigin {
    #[must_use]
    pub const fn new(east_m: CoordinateMeters, north_m: CoordinateMeters) -> Self {
        Self { east_m, north_m }
    }

    #[must_use]
    pub const fn east_m(self) -> CoordinateMeters {
        self.east_m
    }

    #[must_use]
    pub const fn north_m(self) -> CoordinateMeters {
        self.north_m
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NamedVerticalDatum(BoundedText<MAX_VERTICAL_DATUM_BYTES>);

impl NamedVerticalDatum {
    pub fn new(name: impl Into<String>) -> Result<Self, DomainError> {
        Ok(Self(BoundedText::try_new(name, "crs.vertical_datum")?))
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "definition")]
pub enum VerticalDatum {
    NotSpecified,
    Named(NamedVerticalDatum),
}

impl VerticalDatum {
    #[must_use]
    pub const fn not_specified() -> Self {
        Self::NotSpecified
    }

    pub fn named(name: impl Into<String>) -> Result<Self, DomainError> {
        NamedVerticalDatum::new(name).map(Self::Named)
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::NotSpecified => None,
            Self::Named(datum) => Some(datum.name()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CrsProvenance {
    source: BoundedText<MAX_PROVENANCE_FIELD_BYTES>,
    transform_method: BoundedText<MAX_PROVENANCE_FIELD_BYTES>,
    axis_order_decision: BoundedText<MAX_PROVENANCE_FIELD_BYTES>,
}

impl CrsProvenance {
    pub fn new(
        source: impl Into<String>,
        transform_method: impl Into<String>,
        axis_order_decision: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            source: BoundedText::try_new(source, "crs.provenance.source")?,
            transform_method: BoundedText::try_new(
                transform_method,
                "crs.provenance.transform_method",
            )?,
            axis_order_decision: BoundedText::try_new(
                axis_order_decision,
                "crs.provenance.axis_order_decision",
            )?,
        })
    }

    #[must_use]
    pub fn source(&self) -> &str {
        self.source.as_str()
    }

    #[must_use]
    pub fn transform_method(&self) -> &str {
        self.transform_method.as_str()
    }

    #[must_use]
    pub fn axis_order_decision(&self) -> &str {
        self.axis_order_decision.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CoordinateReference {
    source: CrsDefinition,
    engineering: EngineeringCrsDescriptor,
    origin: LocalOrigin,
    axis_order: AxisOrder,
    vertical_datum: VerticalDatum,
    provenance: CrsProvenance,
}

impl CoordinateReference {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source: CrsDefinition,
        engineering: EngineeringCrsDescriptor,
        origin: LocalOrigin,
        axis_order: AxisOrder,
        vertical_datum: VerticalDatum,
        provenance: CrsProvenance,
    ) -> Self {
        Self {
            source,
            engineering,
            origin,
            axis_order,
            vertical_datum,
            provenance,
        }
    }

    #[must_use]
    pub const fn engineering(&self) -> &EngineeringCrsDescriptor {
        &self.engineering
    }

    #[must_use]
    pub const fn origin(&self) -> LocalOrigin {
        self.origin
    }

    #[must_use]
    pub const fn axis_order(&self) -> AxisOrder {
        self.axis_order
    }

    #[must_use]
    pub const fn vertical_datum(&self) -> &VerticalDatum {
        &self.vertical_datum
    }

    #[must_use]
    pub const fn provenance(&self) -> &CrsProvenance {
        &self.provenance
    }

    #[must_use]
    pub const fn source(&self) -> &CrsDefinition {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Project {
    id: ProjectId,
    metadata: ProjectMetadata,
    coordinate_reference: CoordinateReference,
    design_catalog: DesignCatalog,
    study_catalog: StudyCatalog,
    rule_configuration: RuleConfiguration,
}

impl Project {
    #[must_use]
    pub const fn new(
        id: ProjectId,
        metadata: ProjectMetadata,
        coordinate_reference: CoordinateReference,
    ) -> Self {
        Self {
            id,
            metadata,
            coordinate_reference,
            design_catalog: DesignCatalog::empty(),
            study_catalog: StudyCatalog::empty(),
            rule_configuration: RuleConfiguration::unconfigured(),
        }
    }

    #[must_use]
    pub const fn with_catalog(
        id: ProjectId,
        metadata: ProjectMetadata,
        coordinate_reference: CoordinateReference,
        design_catalog: DesignCatalog,
    ) -> Self {
        Self {
            id,
            metadata,
            coordinate_reference,
            design_catalog,
            study_catalog: StudyCatalog::empty(),
            rule_configuration: RuleConfiguration::unconfigured(),
        }
    }

    pub fn with_study_catalog(
        mut self,
        study_catalog: StudyCatalog,
    ) -> Result<Self, ScenarioError> {
        study_catalog.validate_against(&self.design_catalog)?;
        self.study_catalog = study_catalog;
        Ok(self)
    }

    #[must_use]
    pub fn with_rule_configuration(mut self, rule_configuration: RuleConfiguration) -> Self {
        self.rule_configuration = rule_configuration;
        self
    }

    #[must_use]
    pub const fn id(&self) -> ProjectId {
        self.id
    }

    #[must_use]
    pub const fn metadata(&self) -> &ProjectMetadata {
        &self.metadata
    }

    #[must_use]
    pub const fn coordinate_reference(&self) -> &CoordinateReference {
        &self.coordinate_reference
    }

    #[must_use]
    pub const fn design_catalog(&self) -> &DesignCatalog {
        &self.design_catalog
    }

    #[must_use]
    pub const fn study_catalog(&self) -> &StudyCatalog {
        &self.study_catalog
    }

    #[must_use]
    pub const fn rule_configuration(&self) -> &RuleConfiguration {
        &self.rule_configuration
    }
}

impl<'de> Deserialize<'de> for Project {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            id: ProjectId,
            metadata: ProjectMetadata,
            coordinate_reference: CoordinateReference,
            design_catalog: DesignCatalog,
            #[serde(default)]
            study_catalog: StudyCatalog,
            #[serde(default)]
            rule_configuration: RuleConfiguration,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::with_catalog(
            wire.id,
            wire.metadata,
            wire.coordinate_reference,
            wire.design_catalog,
        )
        .with_study_catalog(wire.study_catalog)
        .map(|project| project.with_rule_configuration(wire.rule_configuration))
        .map_err(serde::de::Error::custom)
    }
}
