//! Deterministic Design Model to Compiled Simulation Network pipeline.
//!
//! The first vertical slice accepts straight corridors with one constant cross
//! section. Unsupported geometry is rejected with a stable diagnostic instead
//! of being simplified silently.

use roadsim_compiled_network::{
    COMPILED_NETWORK_SCHEMA_VERSION, CapabilityId, CapabilityRequirements, CompiledLaneUse,
    CompiledNetwork, CompiledNetworkHeader, CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_domain::{
    Corridor, LaneDirection, LaneSlice, LaneUse, Project, ReferenceLineElementKind,
};
use roadsim_types::{ObjectRef, Sha256Digest};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, error::Error, fmt};

/// Stable compilation failure classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileErrorCode {
    EmptyNetwork,
    UnsupportedReferenceElement,
    VariableCrossSectionUnsupported,
    LaneTableInvariant,
    NetworkInvariant,
}

impl CompileErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyNetwork => "compiler.network.empty",
            Self::UnsupportedReferenceElement => "compiler.reference_line.element_unsupported",
            Self::VariableCrossSectionUnsupported => {
                "compiler.corridor.variable_cross_section_unsupported"
            }
            Self::LaneTableInvariant => "compiler.lane_table.invariant",
            Self::NetworkInvariant => "compiler.network.invariant",
        }
    }
}

/// Compilation diagnostic with stable Design Model object references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    code: CompileErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl CompileError {
    fn new(code: CompileErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> CompileErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for CompileError {}

#[derive(Default)]
struct LaneBuilder {
    starts: Vec<CompiledPoint>,
    ends: Vec<CompiledPoint>,
    widths_m: Vec<f64>,
    uses: Vec<CompiledLaneUse>,
    origins: Vec<LaneOrigin>,
    requirements: BTreeSet<CapabilityId>,
}

/// Compiles a complete immutable CSN or returns without publishing partial state.
pub fn compile_project(
    project: &Project,
    source_revision: SourceRevision,
) -> Result<CompiledNetwork, CompileError> {
    let mut builder = LaneBuilder::default();
    for corridor in project.design_catalog().corridors() {
        compile_corridor(corridor, &mut builder)?;
    }
    if builder.starts.is_empty() {
        return Err(CompileError::new(
            CompileErrorCode::EmptyNetwork,
            vec![project.id().into()],
        ));
    }

    let lanes = LaneTable::new(builder.starts, builder.ends, builder.widths_m, builder.uses)
        .ok_or_else(|| CompileError::new(CompileErrorCode::LaneTableInvariant, Vec::new()))?;
    let requirements = CapabilityRequirements::new(builder.requirements);
    let hash = content_hash(project, &lanes, &builder.origins, &requirements);
    CompiledNetwork::new(
        CompiledNetworkHeader::new(source_revision, hash),
        lanes,
        builder.origins,
        requirements,
    )
    .map_err(|_| CompileError::new(CompileErrorCode::NetworkInvariant, Vec::new()))
}

fn compile_corridor(corridor: &Corridor, output: &mut LaneBuilder) -> Result<(), CompileError> {
    if corridor.cross_section_profile().sections().len() != 1 {
        return Err(CompileError::new(
            CompileErrorCode::VariableCrossSectionUnsupported,
            vec![corridor.id().into()],
        ));
    }
    if corridor
        .reference_line()
        .segments()
        .iter()
        .any(|segment| segment.kind() != ReferenceLineElementKind::Line)
    {
        return Err(CompileError::new(
            CompileErrorCode::UnsupportedReferenceElement,
            vec![corridor.id().into()],
        ));
    }

    let start_pose = corridor.reference_line().start();
    let heading = start_pose.heading().get();
    let length = corridor.reference_line().total_length().get();
    let reference_start = start_pose.position();
    // `libm` keeps reference-line evaluation independent of platform libm.
    let tangent = (libm::cos(heading), libm::sin(heading));
    let left_normal = (-tangent.1, tangent.0);
    let reference_end = (
        reference_start.x_m().get() + tangent.0 * length,
        reference_start.y_m().get() + tangent.1 * length,
    );
    let layout = corridor.cross_section_profile().sections()[0].layout();
    compile_side(
        corridor,
        layout.left(),
        1.0,
        reference_end,
        left_normal,
        output,
    );
    compile_side(
        corridor,
        layout.right(),
        -1.0,
        reference_end,
        left_normal,
        output,
    );
    Ok(())
}

fn compile_side(
    corridor: &Corridor,
    slices: &[LaneSlice],
    side_sign: f64,
    reference_end: (f64, f64),
    left_normal: (f64, f64),
    output: &mut LaneBuilder,
) {
    let reference_start = corridor.reference_line().start().position();
    let mut outward_width = 0.0;
    for slice in slices {
        let definition = corridor
            .lane(slice.lane_id())
            .expect("Corridor constructor validates lane references");
        let center_offset = side_sign * (outward_width + slice.width().get() * 0.5);
        outward_width += slice.width().get();
        let start = CompiledPoint::new(
            reference_start.x_m().get() + left_normal.0 * center_offset,
            reference_start.y_m().get() + left_normal.1 * center_offset,
        );
        let end = CompiledPoint::new(
            reference_end.0 + left_normal.0 * center_offset,
            reference_end.1 + left_normal.1 * center_offset,
        );
        let (start, end) = match definition.direction() {
            LaneDirection::AlongReference => (start, end),
            LaneDirection::AgainstReference => (end, start),
        };
        let use_kind = compiled_use(definition.use_kind());
        output.starts.push(start);
        output.ends.push(end);
        output.widths_m.push(slice.width().get());
        output.uses.push(use_kind);
        output
            .origins
            .push(LaneOrigin::new(corridor.id(), definition.id()));
        output.requirements.insert(capability_for(use_kind));
    }
}

const fn compiled_use(use_kind: LaneUse) -> CompiledLaneUse {
    match use_kind {
        LaneUse::GeneralTraffic => CompiledLaneUse::GeneralTraffic,
        LaneUse::BusOnly => CompiledLaneUse::BusOnly,
        LaneUse::Bicycle => CompiledLaneUse::Bicycle,
        LaneUse::Parking => CompiledLaneUse::Parking,
    }
}

const fn capability_for(use_kind: CompiledLaneUse) -> CapabilityId {
    match use_kind {
        CompiledLaneUse::GeneralTraffic => CapabilityId::RoadVehiclesBasic,
        CompiledLaneUse::BusOnly => CapabilityId::TransitBusLanes,
        CompiledLaneUse::Bicycle => CapabilityId::BicycleLanes,
        CompiledLaneUse::Parking => CapabilityId::ParkingLanes,
    }
}

fn content_hash(
    project: &Project,
    lanes: &LaneTable,
    origins: &[LaneOrigin],
    requirements: &CapabilityRequirements,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(COMPILED_NETWORK_SCHEMA_VERSION.to_le_bytes());
    hash.update(project.id().as_uuid().as_bytes());
    hash.update((lanes.len() as u64).to_le_bytes());
    for (lane, origin) in lanes.iter().zip(origins) {
        hash.update(lane.start().x_m().to_bits().to_le_bytes());
        hash.update(lane.start().y_m().to_bits().to_le_bytes());
        hash.update(lane.end().x_m().to_bits().to_le_bytes());
        hash.update(lane.end().y_m().to_bits().to_le_bytes());
        hash.update(lane.width_m().to_bits().to_le_bytes());
        hash.update([lane_use_tag(lane.use_kind())]);
        hash.update(origin.corridor_id().as_uuid().as_bytes());
        hash.update(origin.lane_id().as_uuid().as_bytes());
    }
    for capability in requirements.values() {
        hash.update([capability_tag(*capability)]);
    }
    Sha256Digest::from_bytes(hash.finalize().into())
}

const fn lane_use_tag(value: CompiledLaneUse) -> u8 {
    match value {
        CompiledLaneUse::GeneralTraffic => 0,
        CompiledLaneUse::BusOnly => 1,
        CompiledLaneUse::Bicycle => 2,
        CompiledLaneUse::Parking => 3,
    }
}

const fn capability_tag(value: CapabilityId) -> u8 {
    match value {
        CapabilityId::RoadVehiclesBasic => 0,
        CapabilityId::TransitBusLanes => 1,
        CapabilityId::BicycleLanes => 2,
        CapabilityId::ParkingLanes => 3,
    }
}
