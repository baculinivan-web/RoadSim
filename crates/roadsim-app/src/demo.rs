use roadsim_compiled_network::{CompiledNetwork, SourceRevision};
use roadsim_compiler::compile_project;
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

pub fn compiled_network() -> Result<CompiledNetwork, String> {
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
    let project = Project::with_catalog(
        ProjectId::from_u128(0x10),
        ProjectMetadata::new("Deterministic straight-road demo")
            .map_err(|error| error.to_string())?,
        coordinate_reference,
        DesignCatalog::new(vec![corridor]).map_err(|error| error.to_string())?,
    );
    compile_project(&project, SourceRevision::new(0)).map_err(|error| error.to_string())
}
