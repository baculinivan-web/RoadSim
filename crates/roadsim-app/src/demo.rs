use roadsim_compiled_network::{CompiledNetwork, SourceRevision};
use roadsim_compiler::{CompileOptions, GeometryContext, TessellationOptions, compile_project};
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CrossSectionLayout,
    CrossSectionProfile, CrossSectionSection, CrsDefinition, CrsProvenance, DesignCatalog,
    EngineeringCrsDescriptor, EngineeringUnit, LaneDefinition, LaneDirection, LaneSlice, LaneUse,
    LocalOrigin, Point2Meters, Project, ProjectMetadata, ReferenceLine, ReferenceLineElement,
    ReferenceLinePose, VerticalDatum,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, HeadingRadians, LaneId, LengthMeters, ProjectId,
};

/// Compiles any Design project with the demo's numerical policy.
pub fn compile(project: &Project) -> Result<CompiledNetwork, String> {
    let options = CompileOptions::new(
        GeometryContext::new(1.0e-6, 1.0e-9, 1.0e-9, 0.05, 100_000)
            .map_err(|error| error.to_string())?,
        TessellationOptions::new(0.02, 16, 10_000).map_err(|error| error.to_string())?,
        8.0,
        1_000_000,
        1_000_000,
    )
    .map_err(|error| error.to_string())?;
    compile_project(
        project,
        SourceRevision::new(project.design_catalog().corridors().len() as u64),
        options,
    )
    .map_err(|error| error.to_string())
}

/// Builds the deterministic straight-road demo Design project.
pub fn project() -> Result<Project, String> {
    let corridor_id = CorridorId::from_u128(0x100);
    let left_lane_id = LaneId::from_u128(0x101);
    let right_lane_id = LaneId::from_u128(0x102);
    let length = |value| LengthMeters::try_new(value).map_err(|error| error.to_string());

    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(-60.0).map_err(|error| error.to_string())?,
                CoordinateMeters::try_new(0.0).map_err(|error| error.to_string())?,
            ),
            HeadingRadians::try_new(0.0).map_err(|error| error.to_string())?,
        ),
        vec![ReferenceLineElement::line(length(120.0)?).map_err(|error| error.to_string())?],
    )
    .map_err(|error| error.to_string())?;
    let profile = CrossSectionProfile::new(vec![CrossSectionSection::new(
        length(0.0)?,
        CrossSectionLayout::new(
            vec![LaneSlice::new(left_lane_id, length(3.5)?).map_err(|error| error.to_string())?],
            vec![LaneSlice::new(right_lane_id, length(3.5)?).map_err(|error| error.to_string())?],
        )
        .map_err(|error| error.to_string())?,
    )])
    .map_err(|error| error.to_string())?;
    let corridor = Corridor::new(
        corridor_id,
        reference_line,
        vec![
            LaneDefinition::new(
                left_lane_id,
                LaneDirection::AgainstReference,
                LaneUse::GeneralTraffic,
            ),
            LaneDefinition::new(
                right_lane_id,
                LaneDirection::AlongReference,
                LaneUse::GeneralTraffic,
            ),
        ],
        profile,
    )
    .map_err(|error| error.to_string())?;

    let source = CrsDefinition::Authority(
        AuthorityCrs::new("LOCAL", "ROADSIM-DEMO").map_err(|error| error.to_string())?,
    );
    let engineering = EngineeringCrsDescriptor::new(source.clone(), EngineeringUnit::Metre)
        .map_err(|error| error.to_string())?;
    let coordinate_reference = CoordinateReference::new(
        source,
        engineering,
        LocalOrigin::new(
            CoordinateMeters::try_new(0.0).map_err(|error| error.to_string())?,
            CoordinateMeters::try_new(0.0).map_err(|error| error.to_string())?,
        ),
        AxisOrder::EastNorth,
        VerticalDatum::not_specified(),
        CrsProvenance::new("built-in demo", "identity", "east-north")
            .map_err(|error| error.to_string())?,
    );
    Ok(Project::with_catalog(
        ProjectId::from_u128(0x10),
        ProjectMetadata::new("Deterministic straight-road demo")
            .map_err(|error| error.to_string())?,
        coordinate_reference,
        DesignCatalog::new(vec![corridor]).map_err(|error| error.to_string())?,
    ))
}

#[cfg(test)]
pub fn compiled_network() -> Result<CompiledNetwork, String> {
    compile(&project()?)
}
