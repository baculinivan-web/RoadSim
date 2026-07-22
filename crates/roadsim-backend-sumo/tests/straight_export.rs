use roadsim_backend_sumo::{
    SUMO_EDGES_FILE, SUMO_NETWORK_EXPORT_VERSION, SUMO_NODES_FILE, SumoAgentId,
    SumoExportErrorCode, SumoRoadExportOptions, export_straight_network,
};
use roadsim_compiled_network::{
    CapabilityId, CapabilityRequirements, CompiledLaneUse, CompiledNetwork, CompiledNetworkHeader,
    CompiledPoint, LaneOrigin, LaneTable, SourceRevision,
};
use roadsim_types::{CorridorId, LaneId, Sha256Digest};
use std::{path::PathBuf, process::Command};

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

#[test]
fn straight_lanes_export_deterministically_with_lossless_mapping() {
    let network = network(CompiledLaneUse::GeneralTraffic);
    let first = export_straight_network(&network, options()).unwrap();
    let second = export_straight_network(&network, options()).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.version(), SUMO_NETWORK_EXPORT_VERSION);
    assert_eq!(first.nodes_xml().matches("<node ").count(), 2);
    assert!(first.nodes_xml().contains("x=\"0\" y=\"0\""));
    assert!(first.nodes_xml().contains("x=\"100\" y=\"0\""));
    assert_eq!(first.edges_xml().matches("<edge ").count(), 2);
    assert!(first.edges_xml().contains("id=\"rs_edge_0\""));
    assert!(first.edges_xml().contains("speed=\"13.89\""));
    assert!(first.edges_xml().contains("width=\"3.5\""));

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
fn unsupported_lane_use_fails_with_design_object_evidence() {
    let network = network(CompiledLaneUse::BusOnly);
    let error = export_straight_network(&network, options()).unwrap_err();
    assert_eq!(error.code(), SumoExportErrorCode::UnsupportedLaneUse);
    assert!(
        error
            .object_refs()
            .contains(&CorridorId::from_u128(10).into())
    );
    assert!(error.object_refs().contains(&LaneId::from_u128(11).into()));
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
#[ignore = "requires ROADSIM_NETCONVERT for exact SUMO 1.27.1"]
fn exact_netconvert_accepts_exported_straight_network() {
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
    let bundle =
        export_straight_network(&network(CompiledLaneUse::GeneralTraffic), options()).unwrap();
    std::fs::write(root.join(SUMO_NODES_FILE), bundle.nodes_xml()).unwrap();
    std::fs::write(root.join(SUMO_EDGES_FILE), bundle.edges_xml()).unwrap();
    let output = Command::new(netconvert)
        .current_dir(&root)
        .args([
            "--node-files",
            SUMO_NODES_FILE,
            "--edge-files",
            SUMO_EDGES_FILE,
            "--output-file",
            "roadsim.net.xml",
            "--no-turnarounds",
            "true",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "netconvert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled_xml = std::fs::read_to_string(root.join("roadsim.net.xml")).unwrap();
    assert!(compiled_xml.contains("<edge id=\"rs_edge_0\""));
    assert!(compiled_xml.contains("<lane id=\"rs_edge_0_0\""));
    assert!(compiled_xml.contains("<edge id=\"rs_edge_1\""));
    assert!(compiled_xml.contains("<lane id=\"rs_edge_1_0\""));
    std::fs::remove_dir_all(root).unwrap();
}
