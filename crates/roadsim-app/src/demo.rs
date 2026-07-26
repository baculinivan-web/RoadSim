use roadsim_compiled_network::{CompiledNetwork, SourceRevision};
use roadsim_compiler::{CompileOptions, GeometryContext, TessellationOptions, compile_project};
use roadsim_domain::{
    AuthorityCrs, AxisOrder, CoordinateReference, Corridor, CorridorEnd, CorridorEndpointRef,
    CrossSectionLayout, CrossSectionProfile, CrossSectionSection, CrsDefinition, CrsProvenance,
    DemandEndpoint, DemandFlow, DemandInterval, DemandMode, DemandProfile, DesignCatalog,
    EngineeringCrsDescriptor, EngineeringUnit, Junction, LaneDefinition, LaneDirection, LaneSlice,
    LaneUse, LocalOrigin, Point2Meters, Project, ProjectMetadata, ReferenceLine,
    ReferenceLineElement, ReferenceLinePose, StudyCatalog, VerticalDatum,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, DemandFlowId, DemandProfileId, DurationSeconds, FlowRatePerHour,
    HeadingRadians, JunctionId, LaneId, LengthMeters, ProjectId,
};

/// Demand profile the demo project authors between its two corridors.
pub const DEMO_DEMAND_PROFILE: u128 = 0x130;

/// One straight two-lane corridor of the demo network.
fn demo_corridor(
    corridor_id: u128,
    left_lane_id: u128,
    right_lane_id: u128,
    start_x: f64,
) -> Result<Corridor, String> {
    let corridor_id = CorridorId::from_u128(corridor_id);
    let left_lane_id = LaneId::from_u128(left_lane_id);
    let right_lane_id = LaneId::from_u128(right_lane_id);
    let length = |value| LengthMeters::try_new(value).map_err(|error| error.to_string());
    let reference_line = ReferenceLine::new(
        ReferenceLinePose::new(
            Point2Meters::new(
                CoordinateMeters::try_new(start_x).map_err(|error| error.to_string())?,
                CoordinateMeters::try_new(0.0).map_err(|error| error.to_string())?,
            ),
            HeadingRadians::try_new(0.0).map_err(|error| error.to_string())?,
        ),
        vec![ReferenceLineElement::line(length(60.0)?).map_err(|error| error.to_string())?],
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
    Corridor::new(
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
    .map_err(|error| error.to_string())
}

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

/// Builds the deterministic demo Design project: two straight corridors
/// joined by one junction, plus an authored car demand profile between them.
pub fn project() -> Result<Project, String> {
    let corridor_a = demo_corridor(0x100, 0x101, 0x102, -60.0)?;
    let corridor_b = demo_corridor(0x110, 0x111, 0x112, 0.0)?;
    let junction = Junction::new(
        JunctionId::from_u128(0x120),
        vec![
            CorridorEndpointRef::new(CorridorId::from_u128(0x100), CorridorEnd::End),
            CorridorEndpointRef::new(CorridorId::from_u128(0x110), CorridorEnd::Start),
        ],
    )
    .map_err(|error| error.to_string())?;
    let demand = DemandProfile::new(
        DemandProfileId::from_u128(DEMO_DEMAND_PROFILE),
        vec![
            DemandFlow::new(
                DemandFlowId::from_u128(0x131),
                DemandMode::Car,
                DemandEndpoint::Corridor(CorridorId::from_u128(0x100)),
                DemandEndpoint::Corridor(CorridorId::from_u128(0x110)),
                vec![
                    DemandInterval::new(
                        DurationSeconds::try_new(0.0).map_err(|error| error.to_string())?,
                        DurationSeconds::try_new(600.0).map_err(|error| error.to_string())?,
                        FlowRatePerHour::try_new(360.0).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                ],
            )
            .map_err(|error| error.to_string())?,
        ],
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
    Project::with_catalog(
        ProjectId::from_u128(0x10),
        ProjectMetadata::new("Deterministic demo intersection")
            .map_err(|error| error.to_string())?,
        coordinate_reference,
        DesignCatalog::with_multimodal(
            vec![corridor_a, corridor_b],
            vec![junction],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .map_err(|error| error.to_string())?,
    )
    .with_study_catalog(
        StudyCatalog::new(vec![demand], vec![], vec![]).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
pub fn compiled_network() -> Result<CompiledNetwork, String> {
    compile(&project()?)
}
