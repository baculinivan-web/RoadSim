use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident, $kind:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn from_u128(value: u128) -> Self {
                Self(Uuid::from_u128(value))
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }

        impl From<$name> for ObjectRef {
            fn from(value: $name) -> Self {
                Self {
                    kind: ObjectKind::$kind,
                    id: value.0,
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Project,
    Corridor,
    Lane,
    Junction,
    Crossing,
    WalkingArea,
    Sidewalk,
    RailAlignment,
    TrafficSign,
    RoadMarking,
    StopLine,
    SignalHead,
    SignalGroup,
    SignalProgram,
    SignalPhase,
    SignalController,
    DemandProfile,
    DemandFlow,
    Scenario,
    Experiment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ObjectRef {
    kind: ObjectKind,
    id: Uuid,
}

impl ObjectRef {
    #[must_use]
    pub const fn kind(self) -> ObjectKind {
        self.kind
    }

    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.id
    }
}

typed_id!(ProjectId, Project);
typed_id!(CorridorId, Corridor);
typed_id!(LaneId, Lane);
typed_id!(JunctionId, Junction);
typed_id!(CrossingId, Crossing);
typed_id!(WalkingAreaId, WalkingArea);
typed_id!(SidewalkId, Sidewalk);
typed_id!(RailAlignmentId, RailAlignment);
typed_id!(TrafficSignId, TrafficSign);
typed_id!(RoadMarkingId, RoadMarking);
typed_id!(StopLineId, StopLine);
typed_id!(SignalHeadId, SignalHead);
typed_id!(SignalGroupId, SignalGroup);
typed_id!(SignalProgramId, SignalProgram);
typed_id!(SignalPhaseId, SignalPhase);
typed_id!(SignalControllerId, SignalController);
typed_id!(DemandProfileId, DemandProfile);
typed_id!(DemandFlowId, DemandFlow);
typed_id!(ScenarioId, Scenario);
typed_id!(ExperimentId, Experiment);
