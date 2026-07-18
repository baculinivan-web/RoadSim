//! Backend-independent RoadSim Design Model contracts.
//!
//! Bounded text invariants limit what can enter the model, but a serde format may
//! allocate its input string before validation. Container and parser byte/depth
//! limits therefore remain mandatory at the storage boundary (`E03`).

mod project;

pub use project::{
    AuthorityCrs, AxisOrder, CoordinateReference, CrsDefinition, CrsProvenance, DomainError,
    DomainErrorCode, EngineeringCrsDescriptor, EngineeringUnit, LocalOrigin, NamedVerticalDatum,
    Project, ProjectMetadata, VerticalDatum, WktCrs,
};
