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

mod ids;
mod units;

pub use ids::{
    CorridorId, CrossingId, JunctionId, LaneId, ObjectKind, ObjectRef, ProjectId, RailAlignmentId,
    RoadMarkingId, ScenarioId, SidewalkId, SignalControllerId, SignalGroupId, SignalHeadId,
    SignalPhaseId, SignalProgramId, StopLineId, TrafficSignId, WalkingAreaId,
};
pub use units::{
    AngleRadians, CoordinateMeters, CurvaturePerMeter, CurvatureTolerancePerMeter, DurationSeconds,
    HeadingRadians, LengthMeters, SimulationTick, SpeedMetersPerSecond, ToleranceMeters,
    ValueError, ValueErrorCode,
};
