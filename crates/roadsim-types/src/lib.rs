//! Backend-independent identifiers and scalar value types.
//!
//! Identifier newtypes intentionally do not convert between entity kinds:
//!
//! ```compile_fail
//! use roadsim_types::{CorridorId, LaneId};
//!
//! fn select_lane(_: LaneId) {}
//! select_lane(CorridorId::from_u128(1));
//! ```

mod hashes;
mod ids;
mod units;

pub use hashes::Sha256Digest;

pub use ids::{
    CorridorId, CrossingId, DemandFlowId, DemandProfileId, ExperimentId, JunctionId, LaneId,
    ObjectKind, ObjectRef, ProjectId, RailAlignmentId, RoadMarkingId, RuleExceptionId, ScenarioId,
    SidewalkId, SignalControllerId, SignalGroupId, SignalHeadId, SignalPhaseId, SignalProgramId,
    StopLineId, TrafficSignId, WalkingAreaId,
};
pub use units::{
    AngleRadians, CoordinateMeters, CurvaturePerMeter, CurvatureTolerancePerMeter, DurationSeconds,
    FlowRatePerHour, HeadingRadians, LengthMeters, RootSeed, SimulationTick, SpeedMetersPerSecond,
    ToleranceMeters, ValueError, ValueErrorCode,
};
