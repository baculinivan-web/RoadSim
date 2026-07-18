use crate::{DesignCatalog, multimodal::duplicate_object_ref};
use roadsim_types::{
    CorridorId, DemandFlowId, DemandProfileId, DurationSeconds, ExperimentId, FlowRatePerHour,
    ObjectRef, RailAlignmentId, RootSeed, ScenarioId, WalkingAreaId,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioErrorCode {
    EmptyReferences,
    DuplicateReference,
    DuplicateEntity,
    SameEndpoint,
    IncompatibleEndpoint,
    InvalidInterval,
    OverlappingIntervals,
    ZeroFlowRate,
    ZeroDuration,
    ZeroStep,
    ZeroSamplingInterval,
    WarmupOutsideDuration,
    SamplingFasterThanStep,
    DanglingDemandEndpoint,
    DanglingDemandProfile,
    DanglingScenario,
}

impl ScenarioErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyReferences => "domain.scenario.references.empty",
            Self::DuplicateReference => "domain.scenario.reference.duplicate",
            Self::DuplicateEntity => "domain.scenario.entity.duplicate",
            Self::SameEndpoint => "domain.demand.endpoints.same",
            Self::IncompatibleEndpoint => "domain.demand.endpoint.incompatible",
            Self::InvalidInterval => "domain.demand.interval.invalid",
            Self::OverlappingIntervals => "domain.demand.interval.overlap",
            Self::ZeroFlowRate => "domain.demand.flow_rate.zero",
            Self::ZeroDuration => "domain.scenario.duration.zero",
            Self::ZeroStep => "domain.scenario.step.zero",
            Self::ZeroSamplingInterval => "domain.scenario.sampling.zero",
            Self::WarmupOutsideDuration => "domain.scenario.warmup.outside_duration",
            Self::SamplingFasterThanStep => "domain.scenario.sampling.faster_than_step",
            Self::DanglingDemandEndpoint => "domain.demand.endpoint.dangling",
            Self::DanglingDemandProfile => "domain.scenario.demand_profile.dangling",
            Self::DanglingScenario => "domain.experiment.scenario.dangling",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioError {
    code: ScenarioErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl ScenarioError {
    fn new(code: ScenarioErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> ScenarioErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for ScenarioError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandMode {
    Car,
    Bus,
    Pedestrian,
    Tram,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum DemandEndpoint {
    Corridor(CorridorId),
    WalkingArea(WalkingAreaId),
    RailAlignment(RailAlignmentId),
}

impl DemandEndpoint {
    #[must_use]
    pub fn object_ref(self) -> ObjectRef {
        match self {
            Self::Corridor(id) => id.into(),
            Self::WalkingArea(id) => id.into(),
            Self::RailAlignment(id) => id.into(),
        }
    }

    const fn accepts(self, mode: DemandMode) -> bool {
        matches!(
            (mode, self),
            (DemandMode::Car | DemandMode::Bus, Self::Corridor(_))
                | (DemandMode::Pedestrian, Self::WalkingArea(_))
                | (DemandMode::Tram, Self::RailAlignment(_))
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DemandInterval {
    start_s: DurationSeconds,
    end_s: DurationSeconds,
    flow_per_hour: FlowRatePerHour,
}

impl DemandInterval {
    pub fn new(
        start_s: DurationSeconds,
        end_s: DurationSeconds,
        flow_per_hour: FlowRatePerHour,
    ) -> Result<Self, ScenarioError> {
        if end_s.get() <= start_s.get() {
            return Err(ScenarioError::new(
                ScenarioErrorCode::InvalidInterval,
                vec![],
            ));
        }
        if flow_per_hour.get() == 0.0 {
            return Err(ScenarioError::new(ScenarioErrorCode::ZeroFlowRate, vec![]));
        }
        Ok(Self {
            start_s,
            end_s,
            flow_per_hour,
        })
    }

    #[must_use]
    pub const fn start(&self) -> DurationSeconds {
        self.start_s
    }
    #[must_use]
    pub const fn end(&self) -> DurationSeconds {
        self.end_s
    }
    #[must_use]
    pub const fn flow_per_hour(&self) -> FlowRatePerHour {
        self.flow_per_hour
    }
}

impl<'de> Deserialize<'de> for DemandInterval {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            start_s: DurationSeconds,
            end_s: DurationSeconds,
            flow_per_hour: FlowRatePerHour,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start_s, wire.end_s, wire.flow_per_hour).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DemandFlow {
    id: DemandFlowId,
    mode: DemandMode,
    origin: DemandEndpoint,
    destination: DemandEndpoint,
    intervals: Vec<DemandInterval>,
}

impl DemandFlow {
    pub fn new(
        id: DemandFlowId,
        mode: DemandMode,
        origin: DemandEndpoint,
        destination: DemandEndpoint,
        mut intervals: Vec<DemandInterval>,
    ) -> Result<Self, ScenarioError> {
        if origin == destination {
            return Err(ScenarioError::new(
                ScenarioErrorCode::SameEndpoint,
                vec![id.into(), origin.object_ref()],
            ));
        }
        if !origin.accepts(mode) || !destination.accepts(mode) {
            return Err(ScenarioError::new(
                ScenarioErrorCode::IncompatibleEndpoint,
                vec![id.into(), origin.object_ref(), destination.object_ref()],
            ));
        }
        if intervals.is_empty() {
            return Err(ScenarioError::new(
                ScenarioErrorCode::EmptyReferences,
                vec![id.into()],
            ));
        }
        intervals.sort_by(|left, right| left.start().get().total_cmp(&right.start().get()));
        if intervals
            .windows(2)
            .any(|pair| pair[1].start().get() < pair[0].end().get())
        {
            return Err(ScenarioError::new(
                ScenarioErrorCode::OverlappingIntervals,
                vec![id.into()],
            ));
        }
        Ok(Self {
            id,
            mode,
            origin,
            destination,
            intervals,
        })
    }

    #[must_use]
    pub const fn id(&self) -> DemandFlowId {
        self.id
    }
    #[must_use]
    pub const fn mode(&self) -> DemandMode {
        self.mode
    }
    #[must_use]
    pub const fn origin(&self) -> DemandEndpoint {
        self.origin
    }
    #[must_use]
    pub const fn destination(&self) -> DemandEndpoint {
        self.destination
    }
    #[must_use]
    pub fn intervals(&self) -> &[DemandInterval] {
        &self.intervals
    }
}

impl<'de> Deserialize<'de> for DemandFlow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: DemandFlowId,
            mode: DemandMode,
            origin: DemandEndpoint,
            destination: DemandEndpoint,
            intervals: Vec<DemandInterval>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.mode,
            wire.origin,
            wire.destination,
            wire.intervals,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DemandProfile {
    id: DemandProfileId,
    flows: Vec<DemandFlow>,
}

impl DemandProfile {
    pub fn new(id: DemandProfileId, mut flows: Vec<DemandFlow>) -> Result<Self, ScenarioError> {
        if flows.is_empty() {
            return Err(ScenarioError::new(
                ScenarioErrorCode::EmptyReferences,
                vec![id.into()],
            ));
        }
        flows.sort_by_key(DemandFlow::id);
        if let Some(duplicate) = duplicate_object_ref(flows.iter().map(DemandFlow::id)) {
            return Err(ScenarioError::new(
                ScenarioErrorCode::DuplicateEntity,
                vec![duplicate],
            ));
        }
        Ok(Self { id, flows })
    }

    #[must_use]
    pub const fn id(&self) -> DemandProfileId {
        self.id
    }
    #[must_use]
    pub fn flows(&self) -> &[DemandFlow] {
        &self.flows
    }
}

impl<'de> Deserialize<'de> for DemandProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: DemandProfileId,
            flows: Vec<DemandFlow>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.flows).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ScenarioTiming {
    warmup_s: DurationSeconds,
    duration_s: DurationSeconds,
    step_s: DurationSeconds,
    output_sampling_s: DurationSeconds,
}

impl ScenarioTiming {
    pub fn new(
        warmup_s: DurationSeconds,
        duration_s: DurationSeconds,
        step_s: DurationSeconds,
        output_sampling_s: DurationSeconds,
    ) -> Result<Self, ScenarioError> {
        let error = if duration_s.get() == 0.0 {
            Some(ScenarioErrorCode::ZeroDuration)
        } else if step_s.get() == 0.0 {
            Some(ScenarioErrorCode::ZeroStep)
        } else if output_sampling_s.get() == 0.0 {
            Some(ScenarioErrorCode::ZeroSamplingInterval)
        } else if warmup_s.get() >= duration_s.get() {
            Some(ScenarioErrorCode::WarmupOutsideDuration)
        } else if output_sampling_s.get() < step_s.get() {
            Some(ScenarioErrorCode::SamplingFasterThanStep)
        } else {
            None
        };
        if let Some(code) = error {
            return Err(ScenarioError::new(code, vec![]));
        }
        Ok(Self {
            warmup_s,
            duration_s,
            step_s,
            output_sampling_s,
        })
    }

    #[must_use]
    pub const fn warmup(&self) -> DurationSeconds {
        self.warmup_s
    }
    #[must_use]
    pub const fn duration(&self) -> DurationSeconds {
        self.duration_s
    }
    #[must_use]
    pub const fn step(&self) -> DurationSeconds {
        self.step_s
    }
    #[must_use]
    pub const fn output_sampling(&self) -> DurationSeconds {
        self.output_sampling_s
    }
}

impl<'de> Deserialize<'de> for ScenarioTiming {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            warmup_s: DurationSeconds,
            duration_s: DurationSeconds,
            step_s: DurationSeconds,
            output_sampling_s: DurationSeconds,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.warmup_s,
            wire.duration_s,
            wire.step_s,
            wire.output_sampling_s,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// A backend-independent run intent.
///
/// `root_seed` is the required default for a single run. An [`Experiment`]
/// supplies an explicit replacement root for each requested replication.
pub struct Scenario {
    id: ScenarioId,
    demand_profile_id: DemandProfileId,
    timing: ScenarioTiming,
    root_seed: RootSeed,
}

impl Scenario {
    #[must_use]
    pub const fn new(
        id: ScenarioId,
        demand_profile_id: DemandProfileId,
        timing: ScenarioTiming,
        root_seed: RootSeed,
    ) -> Self {
        Self {
            id,
            demand_profile_id,
            timing,
            root_seed,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ScenarioId {
        self.id
    }
    #[must_use]
    pub const fn demand_profile_id(&self) -> DemandProfileId {
        self.demand_profile_id
    }
    #[must_use]
    pub const fn timing(&self) -> ScenarioTiming {
        self.timing
    }
    #[must_use]
    pub const fn root_seed(&self) -> RootSeed {
        self.root_seed
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// A deterministic batch over scenarios and an explicit set of root seeds.
/// Scenario and seed order is not semantic and is canonicalized by ID/value.
pub struct Experiment {
    id: ExperimentId,
    scenario_ids: Vec<ScenarioId>,
    replication_seeds: Vec<RootSeed>,
}

impl Experiment {
    pub fn new(
        id: ExperimentId,
        mut scenario_ids: Vec<ScenarioId>,
        mut replication_seeds: Vec<RootSeed>,
    ) -> Result<Self, ScenarioError> {
        if scenario_ids.is_empty() || replication_seeds.is_empty() {
            return Err(ScenarioError::new(
                ScenarioErrorCode::EmptyReferences,
                vec![id.into()],
            ));
        }
        scenario_ids.sort_unstable();
        if scenario_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ScenarioError::new(
                ScenarioErrorCode::DuplicateReference,
                vec![id.into()],
            ));
        }
        replication_seeds.sort_unstable();
        if replication_seeds.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ScenarioError::new(
                ScenarioErrorCode::DuplicateReference,
                vec![id.into()],
            ));
        }
        Ok(Self {
            id,
            scenario_ids,
            replication_seeds,
        })
    }

    #[must_use]
    pub const fn id(&self) -> ExperimentId {
        self.id
    }
    #[must_use]
    pub fn scenario_ids(&self) -> &[ScenarioId] {
        &self.scenario_ids
    }
    #[must_use]
    pub fn replication_seeds(&self) -> &[RootSeed] {
        &self.replication_seeds
    }
}

impl<'de> Deserialize<'de> for Experiment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: ExperimentId,
            scenario_ids: Vec<ScenarioId>,
            replication_seeds: Vec<RootSeed>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.scenario_ids, wire.replication_seeds)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StudyCatalog {
    demand_profiles: Vec<DemandProfile>,
    scenarios: Vec<Scenario>,
    experiments: Vec<Experiment>,
}

impl StudyCatalog {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            demand_profiles: Vec::new(),
            scenarios: Vec::new(),
            experiments: Vec::new(),
        }
    }

    pub fn new(
        mut demand_profiles: Vec<DemandProfile>,
        mut scenarios: Vec<Scenario>,
        mut experiments: Vec<Experiment>,
    ) -> Result<Self, ScenarioError> {
        demand_profiles.sort_by_key(DemandProfile::id);
        scenarios.sort_by_key(Scenario::id);
        experiments.sort_by_key(Experiment::id);
        let duplicate = [
            duplicate_object_ref(demand_profiles.iter().map(DemandProfile::id)),
            duplicate_object_ref(scenarios.iter().map(Scenario::id)),
            duplicate_object_ref(experiments.iter().map(Experiment::id)),
            duplicate_object_ref(
                demand_profiles
                    .iter()
                    .flat_map(|profile| profile.flows().iter().map(DemandFlow::id)),
            ),
        ]
        .into_iter()
        .flatten()
        .next();
        if let Some(duplicate) = duplicate {
            return Err(ScenarioError::new(
                ScenarioErrorCode::DuplicateEntity,
                vec![duplicate],
            ));
        }
        Ok(Self {
            demand_profiles,
            scenarios,
            experiments,
        })
    }

    pub(crate) fn validate_against(&self, design: &DesignCatalog) -> Result<(), ScenarioError> {
        for profile in &self.demand_profiles {
            for flow in profile.flows() {
                for endpoint in [flow.origin(), flow.destination()] {
                    let exists = match endpoint {
                        DemandEndpoint::Corridor(id) => design.corridor(id).is_some(),
                        DemandEndpoint::WalkingArea(id) => {
                            design.walking_areas().iter().any(|area| area.id() == id)
                        }
                        DemandEndpoint::RailAlignment(id) => design
                            .rail_alignments()
                            .iter()
                            .any(|alignment| alignment.id() == id),
                    };
                    if !exists {
                        return Err(ScenarioError::new(
                            ScenarioErrorCode::DanglingDemandEndpoint,
                            vec![flow.id().into(), endpoint.object_ref()],
                        ));
                    }
                }
            }
        }

        let profile_ids: BTreeSet<_> = self.demand_profiles.iter().map(DemandProfile::id).collect();
        for scenario in &self.scenarios {
            if !profile_ids.contains(&scenario.demand_profile_id()) {
                return Err(ScenarioError::new(
                    ScenarioErrorCode::DanglingDemandProfile,
                    vec![scenario.id().into(), scenario.demand_profile_id().into()],
                ));
            }
        }

        let scenario_ids: BTreeSet<_> = self.scenarios.iter().map(Scenario::id).collect();
        for experiment in &self.experiments {
            for scenario_id in experiment.scenario_ids() {
                if !scenario_ids.contains(scenario_id) {
                    return Err(ScenarioError::new(
                        ScenarioErrorCode::DanglingScenario,
                        vec![experiment.id().into(), (*scenario_id).into()],
                    ));
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn demand_profiles(&self) -> &[DemandProfile] {
        &self.demand_profiles
    }
    #[must_use]
    pub fn scenarios(&self) -> &[Scenario] {
        &self.scenarios
    }
    #[must_use]
    pub fn experiments(&self) -> &[Experiment] {
        &self.experiments
    }
}

impl<'de> Deserialize<'de> for StudyCatalog {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            demand_profiles: Vec<DemandProfile>,
            scenarios: Vec<Scenario>,
            experiments: Vec<Experiment>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.demand_profiles, wire.scenarios, wire.experiments)
            .map_err(serde::de::Error::custom)
    }
}
