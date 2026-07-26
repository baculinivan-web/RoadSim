use roadsim_backend_sumo::{
    SUMO_CONNECTIONS_FILE, SUMO_EDGES_FILE, SUMO_NETCONVERT_INPUT_ARGUMENTS,
    SUMO_NETWORK_EXPORT_VERSION, SUMO_NODES_FILE, SumoAgentId, SumoExportErrorCode,
    SumoRoadExportOptions, export_network,
};
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledControlTable, CompiledLaneId, CompiledLaneUse,
    CompiledMovement, CompiledMovementCurve, CompiledMovementId, CompiledNetwork,
    CompiledNetworkHeader, CompiledPoint, CompiledSignalController, CompiledSignalGroup,
    CompiledSignalIndication, CompiledSignalPhase, CompiledSignalProgram, CompiledSignalState,
    CompiledStopPosition, CompiledTopology, LaneAdjacency, LaneGraph, LaneOrigin, LaneTable,
    MovementGeometryTable, MovementTable, PedestrianGraph, PedestrianNodeOrigin, SourceRevision,
};
use roadsim_types::{
    CorridorId, JunctionId, LaneId, Sha256Digest, SignalControllerId, SignalGroupId, SignalPhaseId,
    SignalProgramId, StopLineId, WalkingAreaId,
};
use std::{path::PathBuf, process::Command};

const JUNCTION: u128 = 30;
/// Lateral offset of one directional lane from its corridor centerline, in metres.
const LANE_OFFSET_M: f64 = 1.75;
const ARM_LENGTH_M: f64 = 50.0;

fn network(lane_use: CompiledLaneUse) -> CompiledNetwork {
    let lanes = LaneTable::new(
        vec![
            CompiledPoint::new(100.0, 0.0),
            CompiledPoint::new(0.0, -0.0),
        ],
        vec![CompiledPoint::new(0.0, 0.0), CompiledPoint::new(100.0, 0.0)],
        vec![3.5, 3.5],
        vec![lane_use, lane_use],
    )
    .unwrap();
    CompiledNetwork::new(
        CompiledNetworkHeader::new(SourceRevision::new(7), Sha256Digest::from_bytes([9; 32])),
        lanes,
        vec![
            LaneOrigin::new(CorridorId::from_u128(10), LaneId::from_u128(11)),
            LaneOrigin::new(CorridorId::from_u128(10), LaneId::from_u128(12)),
        ],
        CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
    )
    .unwrap()
}

fn options() -> SumoRoadExportOptions {
    SumoRoadExportOptions::new(13.89).unwrap()
}

/// Compact lane IDs of the four-arm fixture, right-hand traffic.
const S_IN: u32 = 0;
const S_OUT: u32 = 1;
const E_IN: u32 = 2;
const E_OUT: u32 = 3;
const N_IN: u32 = 4;
const N_OUT: u32 = 5;
const W_IN: u32 = 6;
const W_OUT: u32 = 7;

/// Right, straight and left turn for every approach; U-turns are not authored.
const TURNS: [(u32, u32); 12] = [
    (S_IN, E_OUT),
    (S_IN, N_OUT),
    (S_IN, W_OUT),
    (E_IN, N_OUT),
    (E_IN, W_OUT),
    (E_IN, S_OUT),
    (N_IN, W_OUT),
    (N_IN, S_OUT),
    (N_IN, E_OUT),
    (W_IN, S_OUT),
    (W_IN, E_OUT),
    (W_IN, N_OUT),
];

fn arm_endpoints() -> Vec<(CompiledPoint, CompiledPoint)> {
    let offset = LANE_OFFSET_M;
    let arm = ARM_LENGTH_M;
    let center = CompiledPoint::new(0.0, 0.0);
    vec![
        (CompiledPoint::new(offset, -arm), center),
        (center, CompiledPoint::new(-offset, -arm)),
        (CompiledPoint::new(arm, offset), center),
        (center, CompiledPoint::new(arm, -offset)),
        (CompiledPoint::new(-offset, arm), center),
        (center, CompiledPoint::new(offset, arm)),
        (CompiledPoint::new(-arm, -offset), center),
        (center, CompiledPoint::new(-arm, offset)),
    ]
}

/// Sorted `(from, to)` pairs, i.e. the compact movement IDs the CSN assigns.
fn sorted_turns() -> Vec<(u32, u32)> {
    let mut turns = TURNS.to_vec();
    turns.sort_unstable();
    turns
}

fn four_arm_network(controls: CompiledControlTable) -> CompiledNetwork {
    four_arm_network_with(
        sorted_turns(),
        controls,
        PedestrianGraph::new(vec![], vec![]).unwrap(),
    )
}

fn four_arm_network_with(
    turns: Vec<(u32, u32)>,
    controls: CompiledControlTable,
    pedestrian_graph: PedestrianGraph,
) -> CompiledNetwork {
    let endpoints = arm_endpoints();
    let lane_count = endpoints.len() as u32;
    let lanes = LaneTable::new(
        endpoints.iter().map(|(start, _)| *start).collect(),
        endpoints.iter().map(|(_, end)| *end).collect(),
        vec![3.5; endpoints.len()],
        vec![CompiledLaneUse::GeneralTraffic; endpoints.len()],
    )
    .unwrap();
    let junction_id = JunctionId::from_u128(JUNCTION);
    let movements: Vec<_> = turns
        .iter()
        .map(|(from, to)| {
            CompiledMovement::new(
                CompiledLaneId::new(*from),
                CompiledLaneId::new(*to),
                junction_id,
            )
        })
        .collect();
    let adjacency: Vec<_> = turns
        .iter()
        .map(|(from, to)| {
            LaneAdjacency::new(
                CompiledLaneId::new(*from),
                CompiledLaneId::new(*to),
                junction_id,
            )
        })
        .collect();
    // The exporter never reads curve control points; the fixture only has to
    // satisfy the CSN invariant of one finite curve per compact movement.
    let curves: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(index, (from, to))| {
            let approach = endpoints[*from as usize].0;
            let departure = endpoints[*to as usize].1;
            CompiledMovementCurve::new(
                CompiledMovementId::new(index as u32),
                CompiledPoint::new(approach.x_m() * 0.1, approach.y_m() * 0.1),
                CompiledPoint::new(0.0, 0.0),
                CompiledPoint::new(0.0, 0.0),
                CompiledPoint::new(departure.x_m() * 0.1, departure.y_m() * 0.1),
            )
        })
        .collect();
    let movement_count = movements.len() as u32;
    CompiledNetwork::new_with_graphs(
        CompiledNetworkHeader::new(SourceRevision::new(7), Sha256Digest::from_bytes([9; 32])),
        lanes,
        (0..endpoints.len())
            .map(|index| {
                LaneOrigin::new(
                    CorridorId::from_u128(100 + (index as u128) / 2),
                    LaneId::from_u128(200 + index as u128),
                )
            })
            .collect(),
        CompiledTopology::new(
            LaneGraph::new(lane_count, adjacency).unwrap(),
            MovementTable::new(lane_count, movements).unwrap(),
            MovementGeometryTable::new(movement_count, curves, Vec::new()).unwrap(),
            pedestrian_graph,
        ),
        controls,
        CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
    )
    .unwrap()
}

fn empty_controls(movement_count: u32) -> CompiledControlTable {
    CompiledControlTable::new(8, movement_count, vec![], vec![], vec![], vec![]).unwrap()
}

fn signalized_controls() -> CompiledControlTable {
    let junction_id = JunctionId::from_u128(JUNCTION);
    let program_id = SignalProgramId::from_u128(40);
    CompiledControlTable::new(
        8,
        TURNS.len() as u32,
        vec![],
        vec![CompiledSignalGroup::new(
            SignalGroupId::from_u128(41),
            junction_id,
            vec![CompiledMovementId::new(0)],
        )],
        vec![CompiledSignalProgram::new(
            program_id,
            junction_id,
            vec![CompiledSignalPhase::new(
                SignalPhaseId::from_u128(42),
                30.0,
                3.0,
                vec![CompiledSignalState::new(
                    roadsim_compiled_network::CompiledSignalGroupId::new(0),
                    CompiledSignalIndication::Green,
                )],
            )],
        )],
        vec![CompiledSignalController::new(
            SignalControllerId::from_u128(43),
            junction_id,
            program_id,
        )],
    )
    .unwrap()
}

#[test]
fn straight_lanes_export_deterministically_with_lossless_mapping() {
    let network = network(CompiledLaneUse::GeneralTraffic);
    let first = export_network(&network, options()).unwrap();
    let second = export_network(&network, options()).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.version(), SUMO_NETWORK_EXPORT_VERSION);
    assert_eq!(first.nodes_xml().matches("<node ").count(), 2);
    assert!(first.nodes_xml().contains("x=\"0\" y=\"0\""));
    assert!(first.nodes_xml().contains("x=\"100\" y=\"0\""));
    assert!(!first.nodes_xml().contains("type=\"priority\""));
    assert_eq!(first.edges_xml().matches("<edge ").count(), 2);
    assert!(first.edges_xml().contains("id=\"rs_edge_0\""));
    assert!(first.edges_xml().contains("speed=\"13.89\""));
    assert!(first.edges_xml().contains("width=\"3.5\""));
    assert_eq!(first.connections_xml().matches("<connection ").count(), 0);
    assert!(first.connection_mappings().is_empty());

    let mapping = &first.lane_mappings()[0];
    assert_eq!(mapping.compiled_lane_id().get(), 0);
    assert_eq!(
        mapping.origin(),
        network.lane_origin(mapping.compiled_lane_id()).unwrap()
    );
    assert_eq!(mapping.edge_id().as_str(), "rs_edge_0");
    assert_eq!(mapping.lane_index(), 0);
}

#[test]
fn four_arm_junction_exports_one_explicit_connection_per_movement() {
    let network = four_arm_network(empty_controls(TURNS.len() as u32));
    let first = export_network(&network, options()).unwrap();
    let second = export_network(&network, options()).unwrap();

    assert_eq!(first, second);
    // Eight arm endpoints plus the single shared junction node.
    assert_eq!(first.nodes_xml().matches("<node ").count(), 9);
    assert_eq!(first.nodes_xml().matches("type=\"priority\"").count(), 1);
    assert_eq!(first.edges_xml().matches("<edge ").count(), 8);
    assert_eq!(
        first.connections_xml().matches("<connection ").count(),
        TURNS.len()
    );
    assert_eq!(first.connection_mappings().len(), TURNS.len());

    for (index, (from, to)) in sorted_turns().into_iter().enumerate() {
        let mapping = &first.connection_mappings()[index];
        assert_eq!(mapping.movement_id().get(), index as u32);
        assert_eq!(mapping.junction_id(), JunctionId::from_u128(JUNCTION));
        assert_eq!(mapping.from_edge_id().as_str(), format!("rs_edge_{from}"));
        assert_eq!(mapping.to_edge_id().as_str(), format!("rs_edge_{to}"));
        assert_eq!(mapping.from_lane_index(), 0);
        assert_eq!(mapping.to_lane_index(), 0);
        assert!(first.connections_xml().contains(&format!(
            "<connection from=\"rs_edge_{from}\" to=\"rs_edge_{to}\" fromLane=\"0\" toLane=\"0\"/>"
        )));
        // Every connection is anchored at the same generated junction node.
        assert_eq!(
            mapping.node_id().as_str(),
            first.connection_mappings()[0].node_id().as_str()
        );
    }
}

#[test]
fn incomplete_junction_movements_fail_instead_of_netconvert_guessing() {
    let mut turns = sorted_turns();
    turns.retain(|(from, _)| *from != W_IN);
    let network = four_arm_network_with(
        turns,
        empty_controls(9),
        PedestrianGraph::new(vec![], vec![]).unwrap(),
    );

    let error = export_network(&network, options()).unwrap_err();
    assert_eq!(
        error.code(),
        SumoExportErrorCode::JunctionMovementsIncomplete
    );
    assert!(
        error
            .object_refs()
            .contains(&LaneId::from_u128(200 + u128::from(W_IN)).into())
    );
    assert!(
        error
            .object_refs()
            .contains(&JunctionId::from_u128(JUNCTION).into())
    );
}

#[test]
fn movement_between_disconnected_lane_endpoints_is_rejected() {
    let lanes = LaneTable::new(
        vec![CompiledPoint::new(0.0, 0.0), CompiledPoint::new(120.0, 0.0)],
        vec![
            CompiledPoint::new(100.0, 0.0),
            CompiledPoint::new(200.0, 0.0),
        ],
        vec![3.5, 3.5],
        vec![
            CompiledLaneUse::GeneralTraffic,
            CompiledLaneUse::GeneralTraffic,
        ],
    )
    .unwrap();
    let junction_id = JunctionId::from_u128(JUNCTION);
    let from = CompiledLaneId::new(0);
    let to = CompiledLaneId::new(1);
    let network = CompiledNetwork::new_with_graphs(
        CompiledNetworkHeader::new(SourceRevision::new(7), Sha256Digest::from_bytes([9; 32])),
        lanes,
        vec![
            LaneOrigin::new(CorridorId::from_u128(10), LaneId::from_u128(11)),
            LaneOrigin::new(CorridorId::from_u128(20), LaneId::from_u128(21)),
        ],
        CompiledTopology::new(
            LaneGraph::new(2, vec![LaneAdjacency::new(from, to, junction_id)]).unwrap(),
            MovementTable::new(2, vec![CompiledMovement::new(from, to, junction_id)]).unwrap(),
            MovementGeometryTable::new(
                1,
                vec![CompiledMovementCurve::new(
                    CompiledMovementId::new(0),
                    CompiledPoint::new(90.0, 0.0),
                    CompiledPoint::new(100.0, 0.0),
                    CompiledPoint::new(110.0, 0.0),
                    CompiledPoint::new(120.0, 0.0),
                )],
                Vec::new(),
            )
            .unwrap(),
            PedestrianGraph::new(Vec::new(), Vec::new()).unwrap(),
        ),
        CompiledControlTable::new(2, 1, vec![], vec![], vec![], vec![]).unwrap(),
        CapabilityRequirements::new([CapabilityId::RoadVehiclesBasic]),
    )
    .unwrap();

    let error = export_network(&network, options()).unwrap_err();
    assert_eq!(
        error.code(),
        SumoExportErrorCode::MovementEndpointsDisconnected
    );
    for object_ref in [
        JunctionId::from_u128(JUNCTION).into(),
        CorridorId::from_u128(10).into(),
        LaneId::from_u128(11).into(),
        CorridorId::from_u128(20).into(),
        LaneId::from_u128(21).into(),
    ] {
        assert!(error.object_refs().contains(&object_ref));
    }
}

#[test]
fn unsupported_lane_use_fails_with_design_object_evidence() {
    let network = network(CompiledLaneUse::BusOnly);
    let error = export_network(&network, options()).unwrap_err();
    assert_eq!(error.code(), SumoExportErrorCode::UnsupportedLaneUse);
    assert!(
        error
            .object_refs()
            .contains(&CorridorId::from_u128(10).into())
    );
    assert!(error.object_refs().contains(&LaneId::from_u128(11).into()));
}

#[test]
fn signal_controls_fail_before_t05_instead_of_unsignalized_downgrade() {
    let network = four_arm_network(signalized_controls());
    let error = export_network(&network, options()).unwrap_err();

    assert_eq!(
        error.code(),
        SumoExportErrorCode::UnsupportedTrafficControls
    );
    assert!(
        error
            .object_refs()
            .contains(&SignalGroupId::from_u128(41).into())
    );
    assert!(
        error
            .object_refs()
            .contains(&SignalControllerId::from_u128(43).into())
    );
}

#[test]
fn stop_positions_fail_before_t05_instead_of_being_dropped() {
    let controls = CompiledControlTable::new(
        8,
        TURNS.len() as u32,
        vec![CompiledStopPosition::new(
            StopLineId::from_u128(50),
            CompiledLaneId::new(S_IN),
            45.0,
        )],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let error = export_network(&four_arm_network(controls), options()).unwrap_err();

    assert_eq!(
        error.code(),
        SumoExportErrorCode::UnsupportedTrafficControls
    );
    assert!(
        error
            .object_refs()
            .contains(&StopLineId::from_u128(50).into())
    );
}

#[test]
fn pedestrian_network_fails_before_t07_instead_of_being_dropped() {
    let network = four_arm_network_with(
        sorted_turns(),
        empty_controls(TURNS.len() as u32),
        PedestrianGraph::new(
            vec![PedestrianNodeOrigin::WalkingArea(WalkingAreaId::from_u128(
                60,
            ))],
            vec![],
        )
        .unwrap(),
    );

    let error = export_network(&network, options()).unwrap_err();
    assert_eq!(
        error.code(),
        SumoExportErrorCode::UnsupportedPedestrianNetwork
    );
}

#[test]
fn speed_is_explicit_and_rejects_invalid_values() {
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let error = SumoRoadExportOptions::new(value).unwrap_err();
        assert_eq!(error.code(), SumoExportErrorCode::InvalidSpeed);
        assert!(error.object_refs().is_empty());
    }
}

#[test]
fn compact_agent_ids_use_the_worker_namespace_without_hashing() {
    assert_eq!(SumoAgentId::from_compact(0).as_str(), "rs_agent_0");
    assert_eq!(
        SumoAgentId::from_compact(u32::MAX).as_str(),
        "rs_agent_4294967295"
    );
}

#[test]
fn netconvert_arguments_disable_invented_connections() {
    assert!(SUMO_NETCONVERT_INPUT_ARGUMENTS.contains(&"--no-turnarounds"));
    assert!(SUMO_NETCONVERT_INPUT_ARGUMENTS.contains(&SUMO_CONNECTIONS_FILE));
}

#[test]
#[ignore = "requires ROADSIM_NETCONVERT for exact SUMO 1.27.1"]
fn exact_netconvert_accepts_exported_four_arm_junction() {
    let netconvert = PathBuf::from(
        std::env::var_os("ROADSIM_NETCONVERT").expect("ROADSIM_NETCONVERT is required"),
    );
    assert!(
        netconvert.is_absolute(),
        "ROADSIM_NETCONVERT must be absolute"
    );
    let root =
        std::env::temp_dir().join(format!("roadsim-sumo-export-smoke-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir(&root).unwrap();
    let bundle = export_network(
        &four_arm_network(empty_controls(TURNS.len() as u32)),
        options(),
    )
    .unwrap();
    std::fs::write(root.join(SUMO_NODES_FILE), bundle.nodes_xml()).unwrap();
    std::fs::write(root.join(SUMO_EDGES_FILE), bundle.edges_xml()).unwrap();
    std::fs::write(root.join(SUMO_CONNECTIONS_FILE), bundle.connections_xml()).unwrap();
    let output = Command::new(netconvert)
        .current_dir(&root)
        .args(SUMO_NETCONVERT_INPUT_ARGUMENTS)
        .args(["--output-file", "roadsim.net.xml"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "netconvert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled_xml = std::fs::read_to_string(root.join("roadsim.net.xml")).unwrap();
    for (from, to) in sorted_turns() {
        assert!(compiled_xml.contains(&format!(
            "<connection from=\"rs_edge_{from}\" to=\"rs_edge_{to}\" fromLane=\"0\" toLane=\"0\""
        )));
    }
    // netconvert must resolve right-of-way for the exported turn paths, so at
    // least one movement yields instead of every approach being major.
    assert!(compiled_xml.contains("<request "));
    std::fs::remove_dir_all(root).unwrap();
}
