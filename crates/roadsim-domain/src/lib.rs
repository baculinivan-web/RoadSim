//! Backend-independent RoadSim Design Model contracts.
//!
//! Bounded text invariants limit what can enter the model, but a serde format may
//! allocate its input string before validation. Container and parser byte/depth
//! limits therefore remain mandatory at the storage boundary (`E03`).

mod corridor;
mod multimodal;
mod project;
mod reference_line;
mod rules;
mod scenario;
mod traffic_control;

pub use corridor::{
    Corridor, CorridorError, CorridorErrorCode, CrossSectionLayout, CrossSectionProfile,
    CrossSectionSection, DesignCatalog, LaneDefinition, LaneDirection, LaneSlice, LaneUse,
};
pub use multimodal::{
    CatalogError, CatalogErrorCode, CorridorEnd, CorridorEndpointRef, CorridorSide, Crossing,
    Junction, MultimodalError, MultimodalErrorCode, RailAlignment, Sidewalk, WalkingArea,
};

pub use project::{
    AuthorityCrs, AxisOrder, CoordinateReference, CrsDefinition, CrsProvenance, DomainError,
    DomainErrorCode, EngineeringCrsDescriptor, EngineeringUnit, LocalOrigin, NamedVerticalDatum,
    Project, ProjectMetadata, VerticalDatum, WktCrs,
};
pub use reference_line::{
    BoundaryContinuity, CircularArcSegment, LineSegment, LinearCurvatureTransition, Point2Meters,
    ReferenceLine, ReferenceLineBoundary, ReferenceLineElement, ReferenceLineElementKind,
    ReferenceLineError, ReferenceLineErrorCode, ReferenceLinePose, StationRange,
};
pub use rules::{
    ExceptionStatus, RuleConfigError, RuleConfigErrorCode, RuleConfiguration, RuleException,
    RulesetPin,
};
pub use scenario::{
    DemandEndpoint, DemandFlow, DemandInterval, DemandMode, DemandProfile, Experiment, Scenario,
    ScenarioError, ScenarioErrorCode, ScenarioTiming, StudyCatalog,
};
pub use traffic_control::{
    ControlCode, ControlError, ControlErrorCode, GroupState, RoadMarking, SignalController,
    SignalGroup, SignalHead, SignalIndication, SignalPhase, SignalProgram, StopLine,
    TrafficControlCatalog, TrafficSign,
};
