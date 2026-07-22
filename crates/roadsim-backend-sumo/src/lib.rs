//! Typed, deterministic CSN to SUMO export contracts.
//!
//! This crate translates immutable backend-independent CSN values. It neither
//! owns a libsumo session nor writes files; the worker/storage boundary decides
//! where the returned bundle is materialized.

use roadsim_compiled_network::{
    CompiledLaneId, CompiledLaneUse, CompiledNetwork, CompiledPoint, LaneOrigin,
};
use roadsim_types::ObjectRef;
use std::{collections::BTreeMap, error::Error, fmt, fmt::Write as _};

/// Version of the typed SUMO plain-network export contract.
pub const SUMO_NETWORK_EXPORT_VERSION: u32 = 1;
/// Conventional filename for the generated SUMO plain nodes document.
pub const SUMO_NODES_FILE: &str = "roadsim.nod.xml";
/// Conventional filename for the generated SUMO plain edges document.
pub const SUMO_EDGES_FILE: &str = "roadsim.edg.xml";
/// Stable namespace used to recover RoadSim compact agent IDs in the worker.
pub const SUMO_AGENT_ID_PREFIX: &str = "rs_agent_";

const MAX_EXPORTED_LANES: usize = 1_000_000;

/// Generated SUMO vehicle/person identifier scoped to one simulation run.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SumoAgentId(String);

impl SumoAgentId {
    #[must_use]
    pub fn from_compact(compact_id: u32) -> Self {
        Self(format!("{SUMO_AGENT_ID_PREFIX}{compact_id}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit backend options that are not yet represented by the CSN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SumoRoadExportOptions {
    speed_mps: f64,
}

impl SumoRoadExportOptions {
    pub fn new(speed_mps: f64) -> Result<Self, SumoExportError> {
        if !speed_mps.is_finite() || speed_mps <= 0.0 {
            return Err(SumoExportError::new(
                SumoExportErrorCode::InvalidSpeed,
                Vec::new(),
            ));
        }
        Ok(Self { speed_mps })
    }

    #[must_use]
    pub const fn speed_mps(self) -> f64 {
        self.speed_mps
    }
}

/// Stable export failure classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SumoExportErrorCode {
    EmptyNetwork,
    NetworkTooLarge,
    InvalidSpeed,
    UnsupportedLaneUse,
}

impl SumoExportErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyNetwork => "backend.sumo.network.empty",
            Self::NetworkTooLarge => "backend.sumo.network.too_large",
            Self::InvalidSpeed => "backend.sumo.road.speed_invalid",
            Self::UnsupportedLaneUse => "backend.sumo.lane_use.unsupported",
        }
    }
}

/// Export diagnostic retaining stable Design Model object references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoExportError {
    code: SumoExportErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl SumoExportError {
    fn new(code: SumoExportErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> SumoExportErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for SumoExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for SumoExportError {}

/// Generated SUMO edge identifier, scoped to one export bundle.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SumoEdgeId(String);

impl SumoEdgeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lossless mapping from one compact CSN lane to generated SUMO identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoLaneMapping {
    compiled_lane_id: CompiledLaneId,
    origin: LaneOrigin,
    edge_id: SumoEdgeId,
    lane_index: u16,
}

impl SumoLaneMapping {
    #[must_use]
    pub const fn compiled_lane_id(&self) -> CompiledLaneId {
        self.compiled_lane_id
    }

    #[must_use]
    pub const fn origin(&self) -> LaneOrigin {
        self.origin
    }

    #[must_use]
    pub const fn edge_id(&self) -> &SumoEdgeId {
        &self.edge_id
    }

    #[must_use]
    pub const fn lane_index(&self) -> u16 {
        self.lane_index
    }
}

/// Complete in-memory plain-network bundle ready for bounded materialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoNetworkBundle {
    version: u32,
    nodes_xml: String,
    edges_xml: String,
    lane_mappings: Vec<SumoLaneMapping>,
}

impl SumoNetworkBundle {
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub fn nodes_xml(&self) -> &str {
        &self.nodes_xml
    }

    #[must_use]
    pub fn edges_xml(&self) -> &str {
        &self.edges_xml
    }

    #[must_use]
    pub fn lane_mappings(&self) -> &[SumoLaneMapping] {
        &self.lane_mappings
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NodeKey {
    x_bits: u64,
    y_bits: u64,
}

impl NodeKey {
    fn from_point(point: CompiledPoint) -> Self {
        Self {
            x_bits: canonical_bits(point.x_m()),
            y_bits: canonical_bits(point.y_m()),
        }
    }

    fn coordinates(self) -> (f64, f64) {
        (f64::from_bits(self.x_bits), f64::from_bits(self.y_bits))
    }
}

/// Translates a CSN straight-lane table without writing files or invoking SUMO.
pub fn export_straight_network(
    network: &CompiledNetwork,
    options: SumoRoadExportOptions,
) -> Result<SumoNetworkBundle, SumoExportError> {
    if network.lanes().is_empty() {
        return Err(SumoExportError::new(
            SumoExportErrorCode::EmptyNetwork,
            Vec::new(),
        ));
    }
    if network.lanes().len() > MAX_EXPORTED_LANES {
        return Err(SumoExportError::new(
            SumoExportErrorCode::NetworkTooLarge,
            Vec::new(),
        ));
    }

    let mut nodes = BTreeMap::<NodeKey, String>::new();
    for lane in network.lanes().iter() {
        let origin = network
            .lane_origin(lane.id())
            .expect("CompiledNetwork guarantees one origin per lane");
        if lane.use_kind() != CompiledLaneUse::GeneralTraffic {
            return Err(SumoExportError::new(
                SumoExportErrorCode::UnsupportedLaneUse,
                vec![origin.corridor_id().into(), origin.lane_id().into()],
            ));
        }
        nodes.entry(NodeKey::from_point(lane.start())).or_default();
        nodes.entry(NodeKey::from_point(lane.end())).or_default();
    }
    for (index, node_id) in nodes.values_mut().enumerate() {
        *node_id = format!("rs_node_{index}");
    }

    let mut nodes_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nodes>\n");
    for (key, node_id) in &nodes {
        let (x_m, y_m) = key.coordinates();
        writeln!(
            nodes_xml,
            "    <node id=\"{node_id}\" x=\"{x_m}\" y=\"{y_m}\"/>"
        )
        .expect("writing to String cannot fail");
    }
    nodes_xml.push_str("</nodes>\n");

    let mut edges_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<edges>\n");
    let mut lane_mappings = Vec::with_capacity(network.lanes().len());
    for lane in network.lanes().iter() {
        let edge_id = SumoEdgeId(format!("rs_edge_{}", lane.id().get()));
        let from = nodes
            .get(&NodeKey::from_point(lane.start()))
            .expect("lane endpoints populated the node map");
        let to = nodes
            .get(&NodeKey::from_point(lane.end()))
            .expect("lane endpoints populated the node map");
        writeln!(
            edges_xml,
            "    <edge id=\"{}\" from=\"{from}\" to=\"{to}\" priority=\"1\" numLanes=\"1\" speed=\"{}\" width=\"{}\" spreadType=\"center\"/>",
            edge_id.as_str(),
            options.speed_mps(),
            lane.width_m(),
        )
        .expect("writing to String cannot fail");
        lane_mappings.push(SumoLaneMapping {
            compiled_lane_id: lane.id(),
            origin: network
                .lane_origin(lane.id())
                .expect("CompiledNetwork guarantees one origin per lane"),
            edge_id,
            lane_index: 0,
        });
    }
    edges_xml.push_str("</edges>\n");

    Ok(SumoNetworkBundle {
        version: SUMO_NETWORK_EXPORT_VERSION,
        nodes_xml,
        edges_xml,
        lane_mappings,
    })
}

fn canonical_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0_f64.to_bits()
    } else {
        value.to_bits()
    }
}
