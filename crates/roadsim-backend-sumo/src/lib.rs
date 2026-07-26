//! Typed, deterministic CSN to SUMO export contracts.
//!
//! This crate translates immutable backend-independent CSN values. It neither
//! owns a libsumo session nor writes files; the worker/storage boundary decides
//! where the returned bundle is materialized.

use roadsim_compiled_network::{
    CompiledLaneId, CompiledLaneUse, CompiledMovementId, CompiledNetwork, CompiledPoint,
    CompiledSignalIndication, LaneOrigin,
};
use roadsim_types::{JunctionId, ObjectRef, SignalControllerId, SignalProgramId};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fmt::Write as _,
};

/// Version of the typed SUMO plain-network export contract.
pub const SUMO_NETWORK_EXPORT_VERSION: u32 = 3;
/// Conventional filename for the generated SUMO plain nodes document.
pub const SUMO_NODES_FILE: &str = "roadsim.nod.xml";
/// Conventional filename for the generated SUMO plain edges document.
pub const SUMO_EDGES_FILE: &str = "roadsim.edg.xml";
/// Conventional filename for the generated SUMO plain connections document.
pub const SUMO_CONNECTIONS_FILE: &str = "roadsim.con.xml";
/// Conventional filename for the generated SUMO traffic light logic document.
pub const SUMO_TLS_FILE: &str = "roadsim.tll.xml";
/// Stable namespace used to recover RoadSim compact agent IDs in the worker.
pub const SUMO_AGENT_ID_PREFIX: &str = "rs_agent_";

/// Input arguments `netconvert` must receive for a bundle to stay lossless.
///
/// The exported connection table is complete for every junction, so
/// `netconvert` must not invent turnarounds or heuristic connections on top of
/// it. Callers append their own `--output-file` and platform arguments.
pub const SUMO_NETCONVERT_INPUT_ARGUMENTS: &[&str] = &[
    "--node-files",
    SUMO_NODES_FILE,
    "--edge-files",
    SUMO_EDGES_FILE,
    "--connection-files",
    SUMO_CONNECTIONS_FILE,
    "--tllogic-files",
    SUMO_TLS_FILE,
    "--no-turnarounds",
    "true",
];

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
    UnsupportedPedestrianNetwork,
    UnsupportedStopPositions,
    UnsupportedSignalIntergreen,
    MovementEndpointsDisconnected,
    JunctionMovementsIncomplete,
    JunctionNodeAmbiguous,
    SignalMovementUnbound,
    SignalJunctionUnknown,
}

impl SumoExportErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyNetwork => "backend.sumo.network.empty",
            Self::NetworkTooLarge => "backend.sumo.network.too_large",
            Self::InvalidSpeed => "backend.sumo.road.speed_invalid",
            Self::UnsupportedLaneUse => "backend.sumo.lane_use.unsupported",
            Self::UnsupportedPedestrianNetwork => "backend.sumo.pedestrian_network.unsupported",
            Self::UnsupportedStopPositions => "backend.sumo.stop_positions.unsupported",
            Self::UnsupportedSignalIntergreen => "backend.sumo.signal_intergreen.unsupported",
            Self::MovementEndpointsDisconnected => "backend.sumo.movement.endpoints_disconnected",
            Self::JunctionMovementsIncomplete => "backend.sumo.junction_movements.incomplete",
            Self::JunctionNodeAmbiguous => "backend.sumo.junction_node.ambiguous",
            Self::SignalMovementUnbound => "backend.sumo.signal_movement.unbound",
            Self::SignalJunctionUnknown => "backend.sumo.signal_junction.unknown",
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

/// Generated SUMO node identifier, scoped to one export bundle.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct SumoNodeId(String);

impl SumoNodeId {
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

/// Lossless mapping from one compact CSN movement to one SUMO connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoConnectionMapping {
    movement_id: CompiledMovementId,
    junction_id: JunctionId,
    node_id: SumoNodeId,
    from_edge_id: SumoEdgeId,
    from_lane_index: u16,
    to_edge_id: SumoEdgeId,
    to_lane_index: u16,
    tls_link: Option<(SumoTlsId, u16)>,
}

impl SumoConnectionMapping {
    #[must_use]
    pub const fn movement_id(&self) -> CompiledMovementId {
        self.movement_id
    }

    #[must_use]
    pub const fn junction_id(&self) -> JunctionId {
        self.junction_id
    }

    #[must_use]
    pub const fn node_id(&self) -> &SumoNodeId {
        &self.node_id
    }

    #[must_use]
    pub const fn from_edge_id(&self) -> &SumoEdgeId {
        &self.from_edge_id
    }

    #[must_use]
    pub const fn from_lane_index(&self) -> u16 {
        self.from_lane_index
    }

    #[must_use]
    pub const fn to_edge_id(&self) -> &SumoEdgeId {
        &self.to_edge_id
    }

    #[must_use]
    pub const fn to_lane_index(&self) -> u16 {
        self.to_lane_index
    }

    /// Generated TLS and `linkIndex` when this movement is signal controlled.
    #[must_use]
    pub fn tls_link(&self) -> Option<(&SumoTlsId, u16)> {
        self.tls_link
            .as_ref()
            .map(|(tls_id, index)| (tls_id, *index))
    }
}

/// Generated SUMO traffic light identifier, scoped to one export bundle.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct SumoTlsId(String);

impl SumoTlsId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lossless mapping from one compiled fixed-time controller to one SUMO TLS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoSignalMapping {
    signal_controller_id: SignalControllerId,
    signal_program_id: SignalProgramId,
    junction_id: JunctionId,
    tls_id: SumoTlsId,
    /// Compact movement per SUMO link index, in exported `linkIndex` order.
    link_movement_ids: Vec<CompiledMovementId>,
}

impl SumoSignalMapping {
    #[must_use]
    pub const fn signal_controller_id(&self) -> SignalControllerId {
        self.signal_controller_id
    }

    #[must_use]
    pub const fn signal_program_id(&self) -> SignalProgramId {
        self.signal_program_id
    }

    #[must_use]
    pub const fn junction_id(&self) -> JunctionId {
        self.junction_id
    }

    #[must_use]
    pub const fn tls_id(&self) -> &SumoTlsId {
        &self.tls_id
    }

    #[must_use]
    pub fn link_movement_ids(&self) -> &[CompiledMovementId] {
        &self.link_movement_ids
    }
}

/// Complete in-memory plain-network bundle ready for bounded materialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumoNetworkBundle {
    version: u32,
    nodes_xml: String,
    edges_xml: String,
    connections_xml: String,
    tls_xml: String,
    lane_mappings: Vec<SumoLaneMapping>,
    connection_mappings: Vec<SumoConnectionMapping>,
    signal_mappings: Vec<SumoSignalMapping>,
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
    pub fn connections_xml(&self) -> &str {
        &self.connections_xml
    }

    #[must_use]
    pub fn tls_xml(&self) -> &str {
        &self.tls_xml
    }

    #[must_use]
    pub fn lane_mappings(&self) -> &[SumoLaneMapping] {
        &self.lane_mappings
    }

    #[must_use]
    pub fn connection_mappings(&self) -> &[SumoConnectionMapping] {
        &self.connection_mappings
    }

    #[must_use]
    pub fn signal_mappings(&self) -> &[SumoSignalMapping] {
        &self.signal_mappings
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

#[derive(Clone, Debug, Default)]
struct NodeRecord {
    id: SumoNodeId,
    incoming: Vec<CompiledLaneId>,
    outgoing: Vec<CompiledLaneId>,
    junction_id: Option<JunctionId>,
}

/// Translates a CSN lane and movement table without writing files or SUMO calls.
///
/// Road lanes become single-lane plain edges, coincident lane endpoints become
/// shared nodes, and every compiled junction movement becomes exactly one
/// explicit SUMO connection. Right-of-way between the exported connections is
/// resolved by `netconvert` from the exported geometry (see ADR-022); RoadSim
/// does not yet author junction priority, so no priority value is invented
/// here. Features the CSN can express but this stage cannot map — pedestrian
/// networks and traffic controls — are rejected with Design object references
/// instead of being dropped.
pub fn export_network(
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
    reject_unsupported_pedestrian_network(network)?;
    reject_unsupported_stop_positions(network)?;

    let mut nodes = BTreeMap::<NodeKey, NodeRecord>::new();
    for lane in network.lanes().iter() {
        let origin = lane_origin(network, lane.id());
        if lane.use_kind() != CompiledLaneUse::GeneralTraffic {
            return Err(SumoExportError::new(
                SumoExportErrorCode::UnsupportedLaneUse,
                vec![origin.corridor_id().into(), origin.lane_id().into()],
            ));
        }
        nodes
            .entry(NodeKey::from_point(lane.start()))
            .or_default()
            .outgoing
            .push(lane.id());
        nodes
            .entry(NodeKey::from_point(lane.end()))
            .or_default()
            .incoming
            .push(lane.id());
    }
    for (index, record) in nodes.values_mut().enumerate() {
        record.id = SumoNodeId(format!("rs_node_{index}"));
    }

    let edge_ids: Vec<SumoEdgeId> = network
        .lanes()
        .iter()
        .map(|lane| SumoEdgeId(format!("rs_edge_{}", lane.id().get())))
        .collect();

    let mut connection_mappings = Vec::with_capacity(network.movements().movements().len());
    let mut connected_incoming = BTreeSet::<CompiledLaneId>::new();
    for (index, movement) in network.movements().movements().iter().enumerate() {
        let movement_id = CompiledMovementId::new(u32::try_from(index).unwrap_or(u32::MAX));
        let from_lane = network
            .lanes()
            .lane(movement.from())
            .expect("CompiledNetwork guarantees movement lanes exist");
        let to_lane = network
            .lanes()
            .lane(movement.to())
            .expect("CompiledNetwork guarantees movement lanes exist");
        let node_key = NodeKey::from_point(from_lane.end());
        if node_key != NodeKey::from_point(to_lane.start()) {
            return Err(SumoExportError::new(
                SumoExportErrorCode::MovementEndpointsDisconnected,
                movement_object_refs(
                    network,
                    movement.junction_id(),
                    movement.from(),
                    movement.to(),
                ),
            ));
        }
        let record = nodes
            .get_mut(&node_key)
            .expect("lane endpoints populated the node map");
        match record.junction_id {
            None => record.junction_id = Some(movement.junction_id()),
            Some(existing) if existing == movement.junction_id() => {}
            Some(existing) => {
                return Err(SumoExportError::new(
                    SumoExportErrorCode::JunctionNodeAmbiguous,
                    vec![existing.into(), movement.junction_id().into()],
                ));
            }
        }
        connected_incoming.insert(movement.from());
        connection_mappings.push(SumoConnectionMapping {
            movement_id,
            junction_id: movement.junction_id(),
            node_id: record.id.clone(),
            from_edge_id: edge_ids[movement.from().get() as usize].clone(),
            from_lane_index: 0,
            to_edge_id: edge_ids[movement.to().get() as usize].clone(),
            to_lane_index: 0,
            tls_link: None,
        });
    }

    let signal_mappings = assign_signal_links(network, &nodes, &mut connection_mappings)?;

    // An exported connection table replaces netconvert's heuristics for the
    // whole junction, so a partially described junction would silently drop
    // real turn paths instead of failing. Turnarounds are excluded because
    // `SUMO_NETCONVERT_INPUT_ARGUMENTS` disables them, so the reverse lane of
    // an approach is not a destination this export can lose.
    for record in nodes.values() {
        if record.outgoing.is_empty() {
            continue;
        }
        for incoming in &record.incoming {
            let incoming_start = NodeKey::from_point(
                network
                    .lanes()
                    .lane(*incoming)
                    .expect("node records only hold table lanes")
                    .start(),
            );
            let has_forward_destination = record.outgoing.iter().any(|outgoing| {
                NodeKey::from_point(
                    network
                        .lanes()
                        .lane(*outgoing)
                        .expect("node records only hold table lanes")
                        .end(),
                ) != incoming_start
            });
            if has_forward_destination && !connected_incoming.contains(incoming) {
                let origin = lane_origin(network, *incoming);
                let mut refs = vec![origin.corridor_id().into(), origin.lane_id().into()];
                if let Some(junction_id) = record.junction_id {
                    refs.push(junction_id.into());
                }
                return Err(SumoExportError::new(
                    SumoExportErrorCode::JunctionMovementsIncomplete,
                    refs,
                ));
            }
        }
    }

    let mut nodes_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nodes>\n");
    for (key, record) in &nodes {
        let (x_m, y_m) = key.coordinates();
        let node_id = record.id.as_str();
        if let Some(tls_id) = record
            .junction_id
            .and_then(|junction_id| tls_id_for_junction(&signal_mappings, junction_id))
        {
            writeln!(
                nodes_xml,
                "    <node id=\"{node_id}\" x=\"{x_m}\" y=\"{y_m}\" type=\"traffic_light\" tl=\"{}\"/>",
                tls_id.as_str()
            )
        } else if record.junction_id.is_some() {
            writeln!(
                nodes_xml,
                "    <node id=\"{node_id}\" x=\"{x_m}\" y=\"{y_m}\" type=\"priority\"/>"
            )
        } else {
            writeln!(
                nodes_xml,
                "    <node id=\"{node_id}\" x=\"{x_m}\" y=\"{y_m}\"/>"
            )
        }
        .expect("writing to String cannot fail");
    }
    nodes_xml.push_str("</nodes>\n");

    let mut edges_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<edges>\n");
    let mut lane_mappings = Vec::with_capacity(network.lanes().len());
    for lane in network.lanes().iter() {
        let edge_id = edge_ids[lane.id().get() as usize].clone();
        let from = nodes
            .get(&NodeKey::from_point(lane.start()))
            .expect("lane endpoints populated the node map")
            .id
            .as_str();
        let to = nodes
            .get(&NodeKey::from_point(lane.end()))
            .expect("lane endpoints populated the node map")
            .id
            .as_str();
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
            origin: lane_origin(network, lane.id()),
            edge_id,
            lane_index: 0,
        });
    }
    edges_xml.push_str("</edges>\n");

    let mut connections_xml =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<connections>\n");
    for mapping in &connection_mappings {
        let tls_attributes = match mapping.tls_link.as_ref() {
            Some((tls_id, link_index)) => {
                format!(" tl=\"{}\" linkIndex=\"{link_index}\"", tls_id.as_str())
            }
            None => String::new(),
        };
        writeln!(
            connections_xml,
            "    <connection from=\"{}\" to=\"{}\" fromLane=\"{}\" toLane=\"{}\"{tls_attributes}/>",
            mapping.from_edge_id.as_str(),
            mapping.to_edge_id.as_str(),
            mapping.from_lane_index,
            mapping.to_lane_index,
        )
        .expect("writing to String cannot fail");
    }
    connections_xml.push_str("</connections>\n");

    let tls_xml = write_tls_document(network, &signal_mappings)?;

    Ok(SumoNetworkBundle {
        version: SUMO_NETWORK_EXPORT_VERSION,
        nodes_xml,
        edges_xml,
        connections_xml,
        tls_xml,
        lane_mappings,
        connection_mappings,
        signal_mappings,
    })
}

fn reject_unsupported_pedestrian_network(network: &CompiledNetwork) -> Result<(), SumoExportError> {
    if network.pedestrian_graph().origins().is_empty() {
        return Ok(());
    }
    Err(SumoExportError::new(
        SumoExportErrorCode::UnsupportedPedestrianNetwork,
        Vec::new(),
    ))
}

/// Authored stop lines are markings; where a vehicle waits inside a SUMO
/// junction is a separate mapping decision and is not part of this stage.
fn reject_unsupported_stop_positions(network: &CompiledNetwork) -> Result<(), SumoExportError> {
    let stop_positions = network.controls().stop_positions();
    if stop_positions.is_empty() {
        return Ok(());
    }
    Err(SumoExportError::new(
        SumoExportErrorCode::UnsupportedStopPositions,
        stop_positions
            .iter()
            .map(|position| position.stop_line_id().into())
            .collect(),
    ))
}

fn tls_id_for_junction(
    signal_mappings: &[SumoSignalMapping],
    junction_id: JunctionId,
) -> Option<&SumoTlsId> {
    signal_mappings
        .iter()
        .find(|mapping| mapping.junction_id == junction_id)
        .map(SumoSignalMapping::tls_id)
}

/// Assigns one deterministic SUMO `linkIndex` per signal-controlled movement.
///
/// Link order follows the compact movement order of the junction, so the state
/// string of every exported phase is stable for one CSN.
fn assign_signal_links(
    network: &CompiledNetwork,
    nodes: &BTreeMap<NodeKey, NodeRecord>,
    connection_mappings: &mut [SumoConnectionMapping],
) -> Result<Vec<SumoSignalMapping>, SumoExportError> {
    let controls = network.controls();
    if controls.signal_controllers().is_empty() {
        // A group without a controller cannot reach this point: the CSN
        // rejects uncontrolled signal groups before the backend sees them.
        return Ok(Vec::new());
    }
    let exported_junctions: BTreeSet<JunctionId> = nodes
        .values()
        .filter_map(|record| record.junction_id)
        .collect();

    let mut signal_mappings = Vec::with_capacity(controls.signal_controllers().len());
    for (index, controller) in controls.signal_controllers().iter().enumerate() {
        if !exported_junctions.contains(&controller.junction_id()) {
            return Err(SumoExportError::new(
                SumoExportErrorCode::SignalJunctionUnknown,
                vec![
                    controller.signal_controller_id().into(),
                    controller.junction_id().into(),
                ],
            ));
        }
        let tls_id = SumoTlsId(format!("rs_tls_{index}"));
        let mut link_movement_ids = Vec::new();
        for mapping in connection_mappings
            .iter_mut()
            .filter(|mapping| mapping.junction_id == controller.junction_id())
        {
            let link_index = u16::try_from(link_movement_ids.len()).map_err(|_| {
                SumoExportError::new(
                    SumoExportErrorCode::NetworkTooLarge,
                    vec![controller.junction_id().into()],
                )
            })?;
            mapping.tls_link = Some((tls_id.clone(), link_index));
            link_movement_ids.push(mapping.movement_id);
        }
        // Every movement of a signalized junction must belong to a group,
        // otherwise the exported state string would silently grant it green.
        for movement_id in &link_movement_ids {
            if !controls
                .signal_groups()
                .iter()
                .any(|group| group.movement_ids().contains(movement_id))
            {
                let movement = network
                    .movements()
                    .movement(*movement_id)
                    .expect("connection mappings only carry table movements");
                return Err(SumoExportError::new(
                    SumoExportErrorCode::SignalMovementUnbound,
                    movement_object_refs(
                        network,
                        movement.junction_id(),
                        movement.from(),
                        movement.to(),
                    ),
                ));
            }
        }
        signal_mappings.push(SumoSignalMapping {
            signal_controller_id: controller.signal_controller_id(),
            signal_program_id: controller.active_program_id(),
            junction_id: controller.junction_id(),
            tls_id,
            link_movement_ids,
        });
    }
    Ok(signal_mappings)
}

/// Serializes the active fixed-time program of every exported controller.
fn write_tls_document(
    network: &CompiledNetwork,
    signal_mappings: &[SumoSignalMapping],
) -> Result<String, SumoExportError> {
    let mut tls_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tlLogics>\n");
    for mapping in signal_mappings {
        let program = network
            .controls()
            .signal_programs()
            .iter()
            .find(|program| program.signal_program_id() == mapping.signal_program_id)
            .expect("CompiledControlTable guarantees the active program exists");
        writeln!(
            tls_xml,
            "    <tlLogic id=\"{}\" type=\"static\" programID=\"rs_program_0\" offset=\"0\">",
            mapping.tls_id.as_str()
        )
        .expect("writing to String cannot fail");
        for phase in program.phases() {
            // Intergreen distributes clearance between amber and all-red, which
            // is a normative interpretation this adapter must not invent.
            if phase.intergreen_s() > 0.0 {
                return Err(SumoExportError::new(
                    SumoExportErrorCode::UnsupportedSignalIntergreen,
                    vec![
                        phase.signal_phase_id().into(),
                        program.signal_program_id().into(),
                    ],
                ));
            }
            let mut state = String::with_capacity(mapping.link_movement_ids.len());
            for movement_id in &mapping.link_movement_ids {
                let group_index = network
                    .controls()
                    .signal_groups()
                    .iter()
                    .position(|group| group.movement_ids().contains(movement_id))
                    .expect("unbound movements were rejected before serialization");
                let indication = phase
                    .states()
                    .iter()
                    .find(|signal_state| signal_state.group_id().get() as usize == group_index)
                    .map(|signal_state| signal_state.indication())
                    .ok_or_else(|| {
                        SumoExportError::new(
                            SumoExportErrorCode::SignalMovementUnbound,
                            vec![phase.signal_phase_id().into(), mapping.junction_id.into()],
                        )
                    })?;
                state.push(indication_character(indication));
            }
            writeln!(
                tls_xml,
                "        <phase duration=\"{}\" state=\"{state}\"/>",
                phase.duration_s()
            )
            .expect("writing to String cannot fail");
        }
        tls_xml.push_str("    </tlLogic>\n");
    }
    tls_xml.push_str("</tlLogics>\n");
    Ok(tls_xml)
}

/// Maps one compiled indication to its SUMO signal-state character.
const fn indication_character(indication: CompiledSignalIndication) -> char {
    match indication {
        // Green is always major: the CSN rejects simultaneously green
        // movements whose geometry conflicts, so no minor green is needed.
        CompiledSignalIndication::Green => 'G',
        CompiledSignalIndication::Amber => 'y',
        CompiledSignalIndication::RedAmber => 'u',
        CompiledSignalIndication::Red => 'r',
        CompiledSignalIndication::Dark => 'O',
    }
}

fn movement_object_refs(
    network: &CompiledNetwork,
    junction_id: JunctionId,
    from: CompiledLaneId,
    to: CompiledLaneId,
) -> Vec<ObjectRef> {
    let from_origin = lane_origin(network, from);
    let to_origin = lane_origin(network, to);
    vec![
        junction_id.into(),
        from_origin.corridor_id().into(),
        from_origin.lane_id().into(),
        to_origin.corridor_id().into(),
        to_origin.lane_id().into(),
    ]
}

fn lane_origin(network: &CompiledNetwork, lane_id: CompiledLaneId) -> LaneOrigin {
    network
        .lane_origin(lane_id)
        .expect("CompiledNetwork guarantees one origin per lane")
}

fn canonical_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0_f64.to_bits()
    } else {
        value.to_bits()
    }
}
