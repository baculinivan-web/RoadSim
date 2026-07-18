use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, CrsDefinition, CrsProvenance, DomainErrorCode,
    EngineeringCrsDescriptor, EngineeringUnit, LocalOrigin, Project, ProjectMetadata,
    VerticalDatum,
};
use roadsim_types::{CoordinateMeters, ProjectId};

fn sample_project() -> Project {
    let source = CrsDefinition::Authority(AuthorityCrs::new("EPSG", "4326").unwrap());
    let engineering_definition =
        CrsDefinition::Authority(AuthorityCrs::new("EPSG", "32637").unwrap());
    let engineering =
        EngineeringCrsDescriptor::new(engineering_definition, EngineeringUnit::Metre).unwrap();
    let origin = LocalOrigin::new(
        CoordinateMeters::try_new(413_000.25).unwrap(),
        CoordinateMeters::try_new(6_176_000.5).unwrap(),
    );
    let provenance = CrsProvenance::new(
        "survey control points",
        "PROJ pipeline to local engineering CRS",
        "explicit east-north order",
    )
    .unwrap();
    let coordinates = CoordinateReference::new(
        source,
        engineering,
        origin,
        AxisOrder::EastNorth,
        VerticalDatum::not_specified(),
        provenance,
    );
    Project::new(
        ProjectId::from_u128(7),
        ProjectMetadata::new("Reference intersection").unwrap(),
        coordinates,
    )
}

#[test]
fn complete_project_metadata_and_crs_round_trip() {
    let project = sample_project();
    let json = serde_json::to_string_pretty(&project).unwrap();
    assert!(json.contains("\"declared_unit\": \"metre\""));
    assert!(json.contains("\"east_m\": 413000.25"));
    assert!(json.contains("\"design_catalog\""));
    assert!(project.design_catalog().corridors().is_empty());
    assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), project);
}

#[test]
fn engineering_crs_rejects_non_metric_units() {
    let definition = CrsDefinition::Authority(AuthorityCrs::new("EPSG", "4326").unwrap());
    let error = EngineeringCrsDescriptor::new(definition, EngineeringUnit::Degree).unwrap_err();
    assert_eq!(error.code(), DomainErrorCode::NonMetricEngineeringCrs);
    assert_eq!(error.field(), "crs.engineering.declared_unit");

    let mut value = serde_json::to_value(sample_project()).unwrap();
    value["coordinate_reference"]["engineering"]["declared_unit"] =
        serde_json::Value::String("degree".to_owned());
    let error = serde_json::from_value::<Project>(value).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("domain.crs.engineering_not_metric")
    );
}

#[test]
fn required_text_is_bounded_on_construction_and_deserialization() {
    assert_eq!(
        ProjectMetadata::new("  ").unwrap_err().code(),
        DomainErrorCode::EmptyText
    );
    assert_eq!(
        ProjectMetadata::new("x".repeat(257)).unwrap_err().code(),
        DomainErrorCode::TextTooLong
    );

    let mut value = serde_json::to_value(sample_project()).unwrap();
    value["metadata"]["name"] = serde_json::Value::String(String::new());
    assert!(serde_json::from_value::<Project>(value).is_err());
}

#[test]
fn coordinate_reference_fields_are_required() {
    let project = serde_json::to_value(sample_project()).unwrap();
    for field in [
        "source",
        "engineering",
        "origin",
        "axis_order",
        "vertical_datum",
        "provenance",
    ] {
        let mut incomplete = project.clone();
        incomplete["coordinate_reference"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<Project>(incomplete).is_err(),
            "missing coordinate_reference.{field} must be rejected"
        );
    }
}

#[test]
fn non_finite_origin_is_rejected_before_project_construction() {
    assert!(CoordinateMeters::try_new(f64::NAN).is_err());
    assert!(CoordinateMeters::try_new(f64::INFINITY).is_err());
}
