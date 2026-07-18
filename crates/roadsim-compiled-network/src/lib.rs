//! Immutable, backend-independent Compiled Simulation Network contracts.
//!
//! These types intentionally contain neither mutable Design Model objects nor
//! UI/backend implementation types. A compiler produces a complete value and
//! only then publishes it to a backend through `Arc<CompiledNetwork>`.

use roadsim_types::{CorridorId, LaneId, Sha256Digest};
use serde::Serialize;
use std::{collections::BTreeSet, error::Error, fmt};

/// Schema of the in-memory CSN contract.
pub const COMPILED_NETWORK_SCHEMA_VERSION: u32 = 1;

/// Monotonic source model revision captured when compilation starts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourceRevision(u64);

impl SourceRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Coordinate frame used by all compiled geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateFrame {
    /// Project-local Cartesian engineering coordinates in metres.
    LocalMetric,
}

/// Stable capability IDs required by a compiled network before a run starts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CapabilityId {
    #[serde(rename = "road.vehicles.basic")]
    RoadVehiclesBasic,
    #[serde(rename = "transit.bus_lanes")]
    TransitBusLanes,
    #[serde(rename = "road.bicycle_lanes")]
    BicycleLanes,
    #[serde(rename = "road.parking_lanes")]
    ParkingLanes,
}

impl CapabilityId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RoadVehiclesBasic => "road.vehicles.basic",
            Self::TransitBusLanes => "transit.bus_lanes",
            Self::BicycleLanes => "road.bicycle_lanes",
            Self::ParkingLanes => "road.parking_lanes",
        }
    }
}

/// Deterministically ordered set of capabilities required by the CSN.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CapabilityRequirements(BTreeSet<CapabilityId>);

impl CapabilityRequirements {
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = CapabilityId>) -> Self {
        Self(values.into_iter().collect())
    }

    #[must_use]
    pub const fn values(&self) -> &BTreeSet<CapabilityId> {
        &self.0
    }

    #[must_use]
    pub fn contains(&self, capability: CapabilityId) -> bool {
        self.0.contains(&capability)
    }
}

/// Compact zero-based lane index used by runtime arrays.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CompiledLaneId(u32);

impl CompiledLaneId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Backend-independent semantic use retained for capability negotiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledLaneUse {
    GeneralTraffic,
    BusOnly,
    Bicycle,
    Parking,
}

/// One point in the local metric CSN coordinate frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CompiledPoint {
    x_m: f64,
    y_m: f64,
}

impl CompiledPoint {
    #[must_use]
    pub const fn new(x_m: f64, y_m: f64) -> Self {
        Self { x_m, y_m }
    }

    #[must_use]
    pub const fn x_m(self) -> f64 {
        self.x_m
    }

    #[must_use]
    pub const fn y_m(self) -> f64 {
        self.y_m
    }
}

/// Origin mapping for one compact compiled lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LaneOrigin {
    corridor_id: CorridorId,
    lane_id: LaneId,
}

impl LaneOrigin {
    #[must_use]
    pub const fn new(corridor_id: CorridorId, lane_id: LaneId) -> Self {
        Self {
            corridor_id,
            lane_id,
        }
    }

    #[must_use]
    pub const fn corridor_id(self) -> CorridorId {
        self.corridor_id
    }

    #[must_use]
    pub const fn lane_id(self) -> LaneId {
        self.lane_id
    }
}

/// Parallel hot arrays for the first compiled road-lane contract.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaneTable {
    starts: Vec<CompiledPoint>,
    ends: Vec<CompiledPoint>,
    widths_m: Vec<f64>,
    uses: Vec<CompiledLaneUse>,
}

impl LaneTable {
    #[must_use]
    pub fn new(
        starts: Vec<CompiledPoint>,
        ends: Vec<CompiledPoint>,
        widths_m: Vec<f64>,
        uses: Vec<CompiledLaneUse>,
    ) -> Option<Self> {
        let len = starts.len();
        if ends.len() != len
            || widths_m.len() != len
            || uses.len() != len
            || len > u32::MAX as usize
            || starts
                .iter()
                .chain(&ends)
                .any(|point| !point.x_m().is_finite() || !point.y_m().is_finite())
            || starts.iter().zip(&ends).any(|(start, end)| start == end)
            || widths_m
                .iter()
                .any(|width| !width.is_finite() || *width <= 0.0)
        {
            return None;
        }
        Some(Self {
            starts,
            ends,
            widths_m,
            uses,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    #[must_use]
    pub fn lane(&self, id: CompiledLaneId) -> Option<CompiledLaneRef> {
        let index = id.get() as usize;
        Some(CompiledLaneRef {
            id,
            start: *self.starts.get(index)?,
            end: *self.ends.get(index)?,
            width_m: *self.widths_m.get(index)?,
            use_kind: *self.uses.get(index)?,
        })
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CompiledLaneRef> + '_ {
        (0..self.len()).map(|index| {
            self.lane(CompiledLaneId::new(index as u32))
                .expect("LaneTable constructor guarantees aligned arrays")
        })
    }
}

/// Read-only view over one row of [`LaneTable`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledLaneRef {
    id: CompiledLaneId,
    start: CompiledPoint,
    end: CompiledPoint,
    width_m: f64,
    use_kind: CompiledLaneUse,
}

impl CompiledLaneRef {
    #[must_use]
    pub const fn id(self) -> CompiledLaneId {
        self.id
    }

    #[must_use]
    pub const fn start(self) -> CompiledPoint {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> CompiledPoint {
        self.end
    }

    #[must_use]
    pub const fn width_m(self) -> f64 {
        self.width_m
    }

    #[must_use]
    pub const fn use_kind(self) -> CompiledLaneUse {
        self.use_kind
    }

    #[must_use]
    pub fn length_m(self) -> f64 {
        (self.end.y_m() - self.start.y_m()).hypot(self.end.x_m() - self.start.x_m())
    }
}

/// Header binds a complete CSN to its exact source observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CompiledNetworkHeader {
    schema_version: u32,
    source_revision: SourceRevision,
    coordinate_frame: CoordinateFrame,
    content_hash: Sha256Digest,
}

impl CompiledNetworkHeader {
    #[must_use]
    pub const fn new(source_revision: SourceRevision, content_hash: Sha256Digest) -> Self {
        Self {
            schema_version: COMPILED_NETWORK_SCHEMA_VERSION,
            source_revision,
            coordinate_frame: CoordinateFrame::LocalMetric,
            content_hash,
        }
    }

    #[must_use]
    pub const fn schema_version(self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn source_revision(self) -> SourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn coordinate_frame(self) -> CoordinateFrame {
        self.coordinate_frame
    }

    #[must_use]
    pub const fn content_hash(self) -> Sha256Digest {
        self.content_hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkErrorCode {
    SourceMapLengthMismatch,
}

impl NetworkErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceMapLengthMismatch => "csn.source_map.length_mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkError(NetworkErrorCode);

impl NetworkError {
    #[must_use]
    pub const fn code(self) -> NetworkErrorCode {
        self.0
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for NetworkError {}

/// Complete immutable network published atomically after successful compilation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompiledNetwork {
    header: CompiledNetworkHeader,
    lanes: LaneTable,
    lane_origins: Vec<LaneOrigin>,
    requirements: CapabilityRequirements,
}

impl CompiledNetwork {
    pub fn new(
        header: CompiledNetworkHeader,
        lanes: LaneTable,
        lane_origins: Vec<LaneOrigin>,
        requirements: CapabilityRequirements,
    ) -> Result<Self, NetworkError> {
        if lanes.len() != lane_origins.len() {
            return Err(NetworkError(NetworkErrorCode::SourceMapLengthMismatch));
        }
        Ok(Self {
            header,
            lanes,
            lane_origins,
            requirements,
        })
    }

    #[must_use]
    pub const fn header(&self) -> CompiledNetworkHeader {
        self.header
    }

    #[must_use]
    pub const fn lanes(&self) -> &LaneTable {
        &self.lanes
    }

    #[must_use]
    pub fn lane_origin(&self, id: CompiledLaneId) -> Option<LaneOrigin> {
        self.lane_origins.get(id.get() as usize).copied()
    }

    #[must_use]
    pub const fn requirements(&self) -> &CapabilityRequirements {
        &self.requirements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_table_rejects_misaligned_or_invalid_arrays() {
        let point = CompiledPoint::new(0.0, 0.0);
        assert!(LaneTable::new(vec![point], vec![], vec![3.5], vec![]).is_none());
        assert!(
            LaneTable::new(
                vec![point],
                vec![CompiledPoint::new(f64::NAN, 0.0)],
                vec![3.5],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
        assert!(
            LaneTable::new(
                vec![point],
                vec![point],
                vec![3.5],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
        assert!(
            LaneTable::new(
                vec![point],
                vec![CompiledPoint::new(1.0, 0.0)],
                vec![0.0],
                vec![CompiledLaneUse::GeneralTraffic],
            )
            .is_none()
        );
    }

    #[test]
    fn network_requires_one_origin_per_compact_lane() {
        let lanes = LaneTable::new(vec![], vec![], vec![], vec![]).unwrap();
        let header =
            CompiledNetworkHeader::new(SourceRevision::new(0), Sha256Digest::from_bytes([0; 32]));
        let error = CompiledNetwork::new(
            header,
            lanes,
            vec![LaneOrigin::new(
                CorridorId::from_u128(1),
                LaneId::from_u128(2),
            )],
            CapabilityRequirements::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), NetworkErrorCode::SourceMapLengthMismatch);
    }

    #[test]
    fn capability_manifest_uses_stable_public_ids() {
        let requirements = CapabilityRequirements::new([
            CapabilityId::TransitBusLanes,
            CapabilityId::RoadVehiclesBasic,
        ]);
        assert_eq!(
            serde_json::to_string(&requirements).unwrap(),
            r#"["road.vehicles.basic","transit.bus_lanes"]"#
        );
    }
}
