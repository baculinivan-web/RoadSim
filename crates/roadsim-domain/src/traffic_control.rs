use crate::{
    CatalogError, CatalogErrorCode, DesignCatalog, Junction, multimodal::duplicate_object_ref,
};
use roadsim_types::{
    CorridorId, DurationSeconds, JunctionId, LaneId, LengthMeters, ObjectRef, RoadMarkingId,
    SignalControllerId, SignalGroupId, SignalHeadId, SignalPhaseId, SignalProgramId, StopLineId,
    TrafficSignId,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlErrorCode {
    EmptyCode,
    CodeTooLong,
    EmptyReferences,
    DuplicateReference,
    DuplicateEntity,
    InvalidStationRange,
    ZeroPhaseDuration,
    PhaseGroupMismatch,
    ActiveProgramMissing,
    InvalidMovementBinding,
}

impl ControlErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyCode => "domain.control.code.empty",
            Self::CodeTooLong => "domain.control.code.too_long",
            Self::EmptyReferences => "domain.control.references.empty",
            Self::DuplicateReference => "domain.control.reference.duplicate",
            Self::DuplicateEntity => "domain.control.entity.duplicate",
            Self::InvalidStationRange => "domain.marking.station_range.invalid",
            Self::ZeroPhaseDuration => "domain.signal.phase.duration.zero",
            Self::PhaseGroupMismatch => "domain.signal.phase.group_mismatch",
            Self::ActiveProgramMissing => "domain.signal.controller.active_program_missing",
            Self::InvalidMovementBinding => "domain.signal.movement_binding.invalid",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlError {
    code: ControlErrorCode,
    object_refs: Vec<ObjectRef>,
}

impl ControlError {
    fn new(code: ControlErrorCode, mut object_refs: Vec<ObjectRef>) -> Self {
        object_refs.sort_unstable();
        object_refs.dedup();
        Self { code, object_refs }
    }

    #[must_use]
    pub const fn code(&self) -> ControlErrorCode {
        self.code
    }

    #[must_use]
    pub fn object_refs(&self) -> &[ObjectRef] {
        &self.object_refs
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for ControlError {}

/// Opaque catalog code. Interpreting a code as a regulatory requirement belongs
/// to a versioned ruleset with official source metadata, not to the Design Model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ControlCode(String);

impl ControlCode {
    pub fn new(value: impl Into<String>) -> Result<Self, ControlError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ControlError::new(ControlErrorCode::EmptyCode, vec![]));
        }
        if value.len() > 128 {
            return Err(ControlError::new(ControlErrorCode::CodeTooLong, vec![]));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ControlCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn sorted_unique<T: Ord + Copy>(
    mut values: Vec<T>,
    owner: ObjectRef,
) -> Result<Vec<T>, ControlError> {
    if values.is_empty() {
        return Err(ControlError::new(
            ControlErrorCode::EmptyReferences,
            vec![owner],
        ));
    }
    values.sort_unstable();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ControlError::new(
            ControlErrorCode::DuplicateReference,
            vec![owner],
        ));
    }
    Ok(values)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrafficSign {
    id: TrafficSignId,
    corridor_id: CorridorId,
    station_m: LengthMeters,
    code: ControlCode,
}

impl TrafficSign {
    #[must_use]
    pub const fn new(
        id: TrafficSignId,
        corridor_id: CorridorId,
        station_m: LengthMeters,
        code: ControlCode,
    ) -> Self {
        Self {
            id,
            corridor_id,
            station_m,
            code,
        }
    }

    #[must_use]
    pub const fn id(&self) -> TrafficSignId {
        self.id
    }
    #[must_use]
    pub const fn corridor_id(&self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn station(&self) -> LengthMeters {
        self.station_m
    }
    #[must_use]
    pub const fn code(&self) -> &ControlCode {
        &self.code
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RoadMarking {
    id: RoadMarkingId,
    corridor_id: CorridorId,
    start_station_m: LengthMeters,
    end_station_m: LengthMeters,
    lane_ids: Vec<LaneId>,
    code: ControlCode,
}

impl RoadMarking {
    pub fn new(
        id: RoadMarkingId,
        corridor_id: CorridorId,
        start_station_m: LengthMeters,
        end_station_m: LengthMeters,
        lane_ids: Vec<LaneId>,
        code: ControlCode,
    ) -> Result<Self, ControlError> {
        if end_station_m.get() <= start_station_m.get() {
            return Err(ControlError::new(
                ControlErrorCode::InvalidStationRange,
                vec![id.into(), corridor_id.into()],
            ));
        }
        Ok(Self {
            id,
            corridor_id,
            start_station_m,
            end_station_m,
            lane_ids: sorted_unique(lane_ids, id.into())?,
            code,
        })
    }

    #[must_use]
    pub const fn id(&self) -> RoadMarkingId {
        self.id
    }
    #[must_use]
    pub const fn corridor_id(&self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn start_station(&self) -> LengthMeters {
        self.start_station_m
    }
    #[must_use]
    pub const fn end_station(&self) -> LengthMeters {
        self.end_station_m
    }
    #[must_use]
    pub fn lane_ids(&self) -> &[LaneId] {
        &self.lane_ids
    }
    #[must_use]
    pub const fn code(&self) -> &ControlCode {
        &self.code
    }
}

impl<'de> Deserialize<'de> for RoadMarking {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: RoadMarkingId,
            corridor_id: CorridorId,
            start_station_m: LengthMeters,
            end_station_m: LengthMeters,
            lane_ids: Vec<LaneId>,
            code: ControlCode,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.corridor_id,
            wire.start_station_m,
            wire.end_station_m,
            wire.lane_ids,
            wire.code,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StopLine {
    id: StopLineId,
    corridor_id: CorridorId,
    station_m: LengthMeters,
    lane_ids: Vec<LaneId>,
}

impl StopLine {
    pub fn new(
        id: StopLineId,
        corridor_id: CorridorId,
        station_m: LengthMeters,
        lane_ids: Vec<LaneId>,
    ) -> Result<Self, ControlError> {
        Ok(Self {
            id,
            corridor_id,
            station_m,
            lane_ids: sorted_unique(lane_ids, id.into())?,
        })
    }

    #[must_use]
    pub const fn id(&self) -> StopLineId {
        self.id
    }
    #[must_use]
    pub const fn corridor_id(&self) -> CorridorId {
        self.corridor_id
    }
    #[must_use]
    pub const fn station(&self) -> LengthMeters {
        self.station_m
    }
    #[must_use]
    pub fn lane_ids(&self) -> &[LaneId] {
        &self.lane_ids
    }
}

impl<'de> Deserialize<'de> for StopLine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: StopLineId,
            corridor_id: CorridorId,
            station_m: LengthMeters,
            lane_ids: Vec<LaneId>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.corridor_id, wire.station_m, wire.lane_ids)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalIndication {
    Red,
    RedAmber,
    Green,
    Amber,
    Dark,
}

/// Authored semantic movement controlled by one signal group.
///
/// Lane IDs remain Design Model IDs; compact movement IDs are assigned only by
/// the compiler after movement inference.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SignalMovementBinding {
    group_id: SignalGroupId,
    from_lane_id: LaneId,
    to_lane_id: LaneId,
}

impl<'de> Deserialize<'de> for SignalMovementBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            group_id: SignalGroupId,
            from_lane_id: LaneId,
            to_lane_id: LaneId,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.group_id, wire.from_lane_id, wire.to_lane_id)
            .map_err(serde::de::Error::custom)
    }
}

impl SignalMovementBinding {
    pub fn new(
        group_id: SignalGroupId,
        from_lane_id: LaneId,
        to_lane_id: LaneId,
    ) -> Result<Self, ControlError> {
        if from_lane_id == to_lane_id {
            return Err(ControlError::new(
                ControlErrorCode::InvalidMovementBinding,
                vec![group_id.into(), from_lane_id.into()],
            ));
        }
        Ok(Self {
            group_id,
            from_lane_id,
            to_lane_id,
        })
    }

    #[must_use]
    pub const fn group_id(self) -> SignalGroupId {
        self.group_id
    }

    #[must_use]
    pub const fn from_lane_id(self) -> LaneId {
        self.from_lane_id
    }

    #[must_use]
    pub const fn to_lane_id(self) -> LaneId {
        self.to_lane_id
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalGroup {
    id: SignalGroupId,
    junction_id: JunctionId,
}

impl SignalGroup {
    #[must_use]
    pub const fn new(id: SignalGroupId, junction_id: JunctionId) -> Self {
        Self { id, junction_id }
    }
    #[must_use]
    pub const fn id(self) -> SignalGroupId {
        self.id
    }
    #[must_use]
    pub const fn junction_id(self) -> JunctionId {
        self.junction_id
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalHead {
    id: SignalHeadId,
    junction_id: JunctionId,
    group_id: SignalGroupId,
}

impl SignalHead {
    #[must_use]
    pub const fn new(id: SignalHeadId, junction_id: JunctionId, group_id: SignalGroupId) -> Self {
        Self {
            id,
            junction_id,
            group_id,
        }
    }
    #[must_use]
    pub const fn id(&self) -> SignalHeadId {
        self.id
    }
    #[must_use]
    pub const fn junction_id(&self) -> JunctionId {
        self.junction_id
    }
    #[must_use]
    pub const fn group_id(&self) -> SignalGroupId {
        self.group_id
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroupState {
    group_id: SignalGroupId,
    indication: SignalIndication,
}

impl GroupState {
    #[must_use]
    pub const fn new(group_id: SignalGroupId, indication: SignalIndication) -> Self {
        Self {
            group_id,
            indication,
        }
    }
    #[must_use]
    pub const fn group_id(self) -> SignalGroupId {
        self.group_id
    }
    #[must_use]
    pub const fn indication(self) -> SignalIndication {
        self.indication
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SignalPhase {
    id: SignalPhaseId,
    duration_s: DurationSeconds,
    intergreen_s: DurationSeconds,
    states: Vec<GroupState>,
}

impl SignalPhase {
    pub fn new(
        id: SignalPhaseId,
        duration_s: DurationSeconds,
        intergreen_s: DurationSeconds,
        mut states: Vec<GroupState>,
    ) -> Result<Self, ControlError> {
        if duration_s.get() == 0.0 {
            return Err(ControlError::new(
                ControlErrorCode::ZeroPhaseDuration,
                vec![id.into()],
            ));
        }
        states.sort_by_key(|state| state.group_id());
        if states.is_empty() {
            return Err(ControlError::new(
                ControlErrorCode::EmptyReferences,
                vec![id.into()],
            ));
        }
        if states
            .windows(2)
            .any(|pair| pair[0].group_id() == pair[1].group_id())
        {
            return Err(ControlError::new(
                ControlErrorCode::DuplicateReference,
                vec![id.into()],
            ));
        }
        Ok(Self {
            id,
            duration_s,
            intergreen_s,
            states,
        })
    }

    #[must_use]
    pub const fn id(&self) -> SignalPhaseId {
        self.id
    }
    #[must_use]
    pub const fn duration(&self) -> DurationSeconds {
        self.duration_s
    }
    #[must_use]
    pub const fn intergreen(&self) -> DurationSeconds {
        self.intergreen_s
    }
    #[must_use]
    pub fn states(&self) -> &[GroupState] {
        &self.states
    }
}

impl<'de> Deserialize<'de> for SignalPhase {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: SignalPhaseId,
            duration_s: DurationSeconds,
            intergreen_s: DurationSeconds,
            states: Vec<GroupState>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.duration_s, wire.intergreen_s, wire.states)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SignalProgram {
    id: SignalProgramId,
    junction_id: JunctionId,
    group_ids: Vec<SignalGroupId>,
    phases: Vec<SignalPhase>,
}

impl SignalProgram {
    pub fn new(
        id: SignalProgramId,
        junction_id: JunctionId,
        group_ids: Vec<SignalGroupId>,
        phases: Vec<SignalPhase>,
    ) -> Result<Self, ControlError> {
        let group_ids = sorted_unique(group_ids, id.into())?;
        if phases.is_empty() {
            return Err(ControlError::new(
                ControlErrorCode::EmptyReferences,
                vec![id.into()],
            ));
        }
        if duplicate_object_ref(phases.iter().map(SignalPhase::id)).is_some() {
            return Err(ControlError::new(
                ControlErrorCode::DuplicateReference,
                vec![id.into()],
            ));
        }
        for phase in &phases {
            let phase_groups: Vec<_> = phase
                .states()
                .iter()
                .map(|state| state.group_id())
                .collect();
            if phase_groups != group_ids {
                return Err(ControlError::new(
                    ControlErrorCode::PhaseGroupMismatch,
                    vec![id.into(), phase.id().into()],
                ));
            }
        }
        Ok(Self {
            id,
            junction_id,
            group_ids,
            phases,
        })
    }

    #[must_use]
    pub const fn id(&self) -> SignalProgramId {
        self.id
    }
    #[must_use]
    pub const fn junction_id(&self) -> JunctionId {
        self.junction_id
    }
    #[must_use]
    pub fn group_ids(&self) -> &[SignalGroupId] {
        &self.group_ids
    }
    #[must_use]
    pub fn phases(&self) -> &[SignalPhase] {
        &self.phases
    }
}

impl<'de> Deserialize<'de> for SignalProgram {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: SignalProgramId,
            junction_id: JunctionId,
            group_ids: Vec<SignalGroupId>,
            phases: Vec<SignalPhase>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.junction_id, wire.group_ids, wire.phases)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SignalController {
    id: SignalControllerId,
    junction_id: JunctionId,
    program_ids: Vec<SignalProgramId>,
    active_program_id: SignalProgramId,
}

impl SignalController {
    pub fn new(
        id: SignalControllerId,
        junction_id: JunctionId,
        program_ids: Vec<SignalProgramId>,
        active_program_id: SignalProgramId,
    ) -> Result<Self, ControlError> {
        let program_ids = sorted_unique(program_ids, id.into())?;
        if !program_ids.contains(&active_program_id) {
            return Err(ControlError::new(
                ControlErrorCode::ActiveProgramMissing,
                vec![id.into(), active_program_id.into()],
            ));
        }
        Ok(Self {
            id,
            junction_id,
            program_ids,
            active_program_id,
        })
    }

    #[must_use]
    pub const fn id(&self) -> SignalControllerId {
        self.id
    }
    #[must_use]
    pub const fn junction_id(&self) -> JunctionId {
        self.junction_id
    }
    #[must_use]
    pub fn program_ids(&self) -> &[SignalProgramId] {
        &self.program_ids
    }
    #[must_use]
    pub const fn active_program_id(&self) -> SignalProgramId {
        self.active_program_id
    }
}

impl<'de> Deserialize<'de> for SignalController {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: SignalControllerId,
            junction_id: JunctionId,
            program_ids: Vec<SignalProgramId>,
            active_program_id: SignalProgramId,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.junction_id,
            wire.program_ids,
            wire.active_program_id,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TrafficControlCatalog {
    signs: Vec<TrafficSign>,
    markings: Vec<RoadMarking>,
    stop_lines: Vec<StopLine>,
    groups: Vec<SignalGroup>,
    heads: Vec<SignalHead>,
    programs: Vec<SignalProgram>,
    controllers: Vec<SignalController>,
    movement_bindings: Vec<SignalMovementBinding>,
}

impl TrafficControlCatalog {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            signs: Vec::new(),
            markings: Vec::new(),
            stop_lines: Vec::new(),
            groups: Vec::new(),
            heads: Vec::new(),
            programs: Vec::new(),
            controllers: Vec::new(),
            movement_bindings: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mut signs: Vec<TrafficSign>,
        mut markings: Vec<RoadMarking>,
        mut stop_lines: Vec<StopLine>,
        mut groups: Vec<SignalGroup>,
        mut heads: Vec<SignalHead>,
        mut programs: Vec<SignalProgram>,
        mut controllers: Vec<SignalController>,
    ) -> Result<Self, ControlError> {
        signs.sort_by_key(TrafficSign::id);
        markings.sort_by_key(RoadMarking::id);
        stop_lines.sort_by_key(StopLine::id);
        groups.sort_by_key(|group| group.id());
        heads.sort_by_key(SignalHead::id);
        programs.sort_by_key(SignalProgram::id);
        controllers.sort_by_key(SignalController::id);

        let duplicate = [
            duplicate_object_ref(signs.iter().map(TrafficSign::id)),
            duplicate_object_ref(markings.iter().map(RoadMarking::id)),
            duplicate_object_ref(stop_lines.iter().map(StopLine::id)),
            duplicate_object_ref(groups.iter().map(|group| group.id())),
            duplicate_object_ref(heads.iter().map(SignalHead::id)),
            duplicate_object_ref(programs.iter().map(SignalProgram::id)),
            duplicate_object_ref(controllers.iter().map(SignalController::id)),
            duplicate_object_ref(
                programs
                    .iter()
                    .flat_map(|program| program.phases().iter().map(SignalPhase::id)),
            ),
        ]
        .into_iter()
        .flatten()
        .next();
        if let Some(duplicate) = duplicate {
            return Err(ControlError::new(
                ControlErrorCode::DuplicateEntity,
                vec![duplicate],
            ));
        }

        Ok(Self {
            signs,
            markings,
            stop_lines,
            groups,
            heads,
            programs,
            controllers,
            movement_bindings: Vec::new(),
        })
    }

    /// Adds explicit signal-group to movement intent without inferring it from
    /// signal head placement or geometry.
    pub fn with_signal_movement_bindings(
        mut self,
        mut movement_bindings: Vec<SignalMovementBinding>,
    ) -> Result<Self, ControlError> {
        movement_bindings.sort_unstable();
        if let Some(duplicate) = movement_bindings
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ControlError::new(
                ControlErrorCode::DuplicateReference,
                vec![
                    duplicate.group_id().into(),
                    duplicate.from_lane_id().into(),
                    duplicate.to_lane_id().into(),
                ],
            ));
        }
        self.movement_bindings = movement_bindings;
        Ok(self)
    }

    pub(crate) fn validate_against(&self, design: &DesignCatalog) -> Result<(), CatalogError> {
        let junction_ids: BTreeSet<_> = design.junctions().iter().map(Junction::id).collect();
        let group_ids: BTreeSet<_> = self.groups.iter().map(|group| group.id()).collect();
        let program_ids: BTreeSet<_> = self.programs.iter().map(SignalProgram::id).collect();

        for sign in &self.signs {
            validate_corridor_station(
                design,
                sign.corridor_id(),
                sign.station(),
                sign.id().into(),
            )?;
        }
        for marking in &self.markings {
            validate_corridor_station(
                design,
                marking.corridor_id(),
                marking.end_station(),
                marking.id().into(),
            )?;
            validate_lanes(
                design,
                marking.corridor_id(),
                marking.lane_ids(),
                marking.id().into(),
            )?;
        }
        for stop_line in &self.stop_lines {
            validate_corridor_station(
                design,
                stop_line.corridor_id(),
                stop_line.station(),
                stop_line.id().into(),
            )?;
            validate_lanes(
                design,
                stop_line.corridor_id(),
                stop_line.lane_ids(),
                stop_line.id().into(),
            )?;
        }
        for group in &self.groups {
            if !junction_ids.contains(&group.junction_id()) {
                return Err(CatalogError::new(
                    CatalogErrorCode::DanglingJunction,
                    vec![group.id().into(), group.junction_id().into()],
                ));
            }
        }
        for head in &self.heads {
            validate_junction(&junction_ids, head.junction_id(), head.id().into())?;
            let group = self.group(head.group_id()).ok_or_else(|| {
                CatalogError::new(
                    CatalogErrorCode::DanglingSignalGroup,
                    vec![head.id().into(), head.group_id().into()],
                )
            })?;
            if group.junction_id() != head.junction_id() {
                return Err(CatalogError::new(
                    CatalogErrorCode::SignalJunctionMismatch,
                    vec![
                        head.id().into(),
                        group.id().into(),
                        head.junction_id().into(),
                    ],
                ));
            }
        }
        for program in &self.programs {
            validate_junction(&junction_ids, program.junction_id(), program.id().into())?;
            for group_id in program.group_ids() {
                if !group_ids.contains(group_id) {
                    return Err(CatalogError::new(
                        CatalogErrorCode::DanglingSignalGroup,
                        vec![program.id().into(), (*group_id).into()],
                    ));
                }
                if self
                    .group(*group_id)
                    .expect("membership checked")
                    .junction_id()
                    != program.junction_id()
                {
                    return Err(CatalogError::new(
                        CatalogErrorCode::SignalJunctionMismatch,
                        vec![
                            program.id().into(),
                            (*group_id).into(),
                            program.junction_id().into(),
                        ],
                    ));
                }
            }
        }
        for controller in &self.controllers {
            validate_junction(
                &junction_ids,
                controller.junction_id(),
                controller.id().into(),
            )?;
            for program_id in controller.program_ids() {
                if !program_ids.contains(program_id) {
                    return Err(CatalogError::new(
                        CatalogErrorCode::DanglingSignalProgram,
                        vec![controller.id().into(), (*program_id).into()],
                    ));
                }
                if self
                    .signal_program(*program_id)
                    .expect("membership checked")
                    .junction_id()
                    != controller.junction_id()
                {
                    return Err(CatalogError::new(
                        CatalogErrorCode::SignalJunctionMismatch,
                        vec![
                            controller.id().into(),
                            (*program_id).into(),
                            controller.junction_id().into(),
                        ],
                    ));
                }
            }
        }
        for binding in &self.movement_bindings {
            if !group_ids.contains(&binding.group_id()) {
                return Err(CatalogError::new(
                    CatalogErrorCode::DanglingSignalGroup,
                    vec![binding.group_id().into()],
                ));
            }
            for lane_id in [binding.from_lane_id(), binding.to_lane_id()] {
                if !design
                    .corridors()
                    .iter()
                    .any(|corridor| corridor.lane(lane_id).is_some())
                {
                    return Err(CatalogError::new(
                        CatalogErrorCode::DanglingLane,
                        vec![binding.group_id().into(), lane_id.into()],
                    ));
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn signs(&self) -> &[TrafficSign] {
        &self.signs
    }
    #[must_use]
    pub fn markings(&self) -> &[RoadMarking] {
        &self.markings
    }
    #[must_use]
    pub fn stop_lines(&self) -> &[StopLine] {
        &self.stop_lines
    }
    #[must_use]
    pub fn signal_groups(&self) -> &[SignalGroup] {
        &self.groups
    }
    #[must_use]
    pub fn signal_heads(&self) -> &[SignalHead] {
        &self.heads
    }
    #[must_use]
    pub fn signal_programs(&self) -> &[SignalProgram] {
        &self.programs
    }
    #[must_use]
    pub fn controllers(&self) -> &[SignalController] {
        &self.controllers
    }
    #[must_use]
    pub fn signal_movement_bindings(&self) -> &[SignalMovementBinding] {
        &self.movement_bindings
    }

    fn group(&self, id: SignalGroupId) -> Option<&SignalGroup> {
        self.groups
            .binary_search_by_key(&id, |group| group.id())
            .ok()
            .map(|index| &self.groups[index])
    }

    fn signal_program(&self, id: SignalProgramId) -> Option<&SignalProgram> {
        self.programs
            .binary_search_by_key(&id, SignalProgram::id)
            .ok()
            .map(|index| &self.programs[index])
    }
}

impl<'de> Deserialize<'de> for TrafficControlCatalog {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            signs: Vec<TrafficSign>,
            markings: Vec<RoadMarking>,
            stop_lines: Vec<StopLine>,
            groups: Vec<SignalGroup>,
            heads: Vec<SignalHead>,
            programs: Vec<SignalProgram>,
            controllers: Vec<SignalController>,
            #[serde(default)]
            movement_bindings: Vec<SignalMovementBinding>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.signs,
            wire.markings,
            wire.stop_lines,
            wire.groups,
            wire.heads,
            wire.programs,
            wire.controllers,
        )
        .and_then(|catalog| catalog.with_signal_movement_bindings(wire.movement_bindings))
        .map_err(serde::de::Error::custom)
    }
}

fn validate_junction(
    junction_ids: &BTreeSet<JunctionId>,
    junction_id: JunctionId,
    owner: ObjectRef,
) -> Result<(), CatalogError> {
    if !junction_ids.contains(&junction_id) {
        return Err(CatalogError::new(
            CatalogErrorCode::DanglingJunction,
            vec![owner, junction_id.into()],
        ));
    }
    Ok(())
}

fn validate_corridor_station(
    design: &DesignCatalog,
    corridor_id: CorridorId,
    station: LengthMeters,
    owner: ObjectRef,
) -> Result<(), CatalogError> {
    let corridor = design.corridor(corridor_id).ok_or_else(|| {
        CatalogError::new(
            CatalogErrorCode::DanglingCorridor,
            vec![owner, corridor_id.into()],
        )
    })?;
    if station.get() >= corridor.reference_line().total_length().get() {
        return Err(CatalogError::new(
            CatalogErrorCode::StationOutsideCorridor,
            vec![owner, corridor_id.into()],
        ));
    }
    Ok(())
}

fn validate_lanes(
    design: &DesignCatalog,
    corridor_id: CorridorId,
    lane_ids: &[LaneId],
    owner: ObjectRef,
) -> Result<(), CatalogError> {
    let corridor = design.corridor(corridor_id).ok_or_else(|| {
        CatalogError::new(
            CatalogErrorCode::DanglingCorridor,
            vec![owner, corridor_id.into()],
        )
    })?;
    for lane_id in lane_ids {
        if corridor.lane(*lane_id).is_none() {
            return Err(CatalogError::new(
                CatalogErrorCode::DanglingLane,
                vec![owner, (*lane_id).into()],
            ));
        }
    }
    Ok(())
}
