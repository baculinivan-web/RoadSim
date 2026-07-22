//! Deterministic local-metric geometry derived from the Design Model.
//!
//! Every operation receives an explicit [`GeometryContext`]. The kernel has no
//! process-global epsilon and never changes authoring geometry.

use roadsim_domain::{
    Corridor, CrossSectionLayout, LaneDirection, LaneSlice, Point2Meters, ReferenceLine,
    ReferenceLineElement,
};
use roadsim_types::{CoordinateMeters, CurvaturePerMeter, HeadingRadians, LaneId, LengthMeters};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

/// Hard resource ceiling independent of caller-selected numerical accuracy.
pub const MAX_INTEGRATION_PANELS: u32 = 1_000_000;
pub const MAX_TESSELLATION_DEPTH: u32 = 24;
pub const MAX_TESSELLATION_POINTS: u32 = 1_000_000;

/// Stable geometry failure classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryErrorCode {
    InvalidDistanceTolerance,
    InvalidAreaTolerance,
    InvalidParameterTolerance,
    InvalidIntegrationStep,
    InvalidIntegrationPanelLimit,
    StationOutsideCurve,
    IntegrationLimitExceeded,
    NonFiniteOffset,
    OffsetSingular,
    InvalidCurveParameter,
    InvalidTessellationError,
    InvalidTessellationDepth,
    InvalidTessellationPointLimit,
    TessellationLimitExceeded,
    NonFiniteResult,
}

impl GeometryErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDistanceTolerance => "geometry.context.distance_tolerance.invalid",
            Self::InvalidAreaTolerance => "geometry.context.area_tolerance.invalid",
            Self::InvalidParameterTolerance => "geometry.context.parameter_tolerance.invalid",
            Self::InvalidIntegrationStep => "geometry.context.integration_step.invalid",
            Self::InvalidIntegrationPanelLimit => "geometry.context.integration_panels.invalid",
            Self::StationOutsideCurve => "geometry.curve.station.outside",
            Self::IntegrationLimitExceeded => "geometry.curve.integration.limit_exceeded",
            Self::NonFiniteOffset => "geometry.offset.non_finite",
            Self::OffsetSingular => "geometry.offset.singular",
            Self::InvalidCurveParameter => "geometry.curve.parameter.invalid",
            Self::InvalidTessellationError => "geometry.tessellation.error.invalid",
            Self::InvalidTessellationDepth => "geometry.tessellation.depth.invalid",
            Self::InvalidTessellationPointLimit => "geometry.tessellation.points.invalid",
            Self::TessellationLimitExceeded => "geometry.tessellation.limit_exceeded",
            Self::NonFiniteResult => "geometry.result.non_finite",
        }
    }
}

/// Exact cubic Bézier control polygon in the local metric coordinate frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier2 {
    start: Point2Meters,
    control_1: Point2Meters,
    control_2: Point2Meters,
    end: Point2Meters,
}

impl CubicBezier2 {
    #[must_use]
    pub const fn new(
        start: Point2Meters,
        control_1: Point2Meters,
        control_2: Point2Meters,
        end: Point2Meters,
    ) -> Self {
        Self {
            start,
            control_1,
            control_2,
            end,
        }
    }

    #[must_use]
    pub const fn start(self) -> Point2Meters {
        self.start
    }

    #[must_use]
    pub const fn control_1(self) -> Point2Meters {
        self.control_1
    }

    #[must_use]
    pub const fn control_2(self) -> Point2Meters {
        self.control_2
    }

    #[must_use]
    pub const fn end(self) -> Point2Meters {
        self.end
    }

    pub fn point_at(self, parameter: f64) -> Result<Point2Meters, GeometryError> {
        if !parameter.is_finite() || !(0.0..=1.0).contains(&parameter) {
            return Err(GeometryError::new(GeometryErrorCode::InvalidCurveParameter));
        }
        let first = interpolate(
            coordinates(self.start),
            coordinates(self.control_1),
            parameter,
        )?;
        let second = interpolate(
            coordinates(self.control_1),
            coordinates(self.control_2),
            parameter,
        )?;
        let third = interpolate(
            coordinates(self.control_2),
            coordinates(self.end),
            parameter,
        )?;
        let fourth = interpolate(first, second, parameter)?;
        let fifth = interpolate(second, third, parameter)?;
        result_point(interpolate(fourth, fifth, parameter)?)
    }
}

/// Explicit bounded tessellation policy. The curve control polygon remains the
/// engineering geometry; returned points are a derived approximation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TessellationOptions {
    max_chord_error_m: f64,
    max_depth: u32,
    max_points: u32,
}

impl TessellationOptions {
    pub fn new(
        max_chord_error_m: f64,
        max_depth: u32,
        max_points: u32,
    ) -> Result<Self, GeometryError> {
        if !max_chord_error_m.is_finite() || max_chord_error_m <= 0.0 {
            return Err(GeometryError::new(
                GeometryErrorCode::InvalidTessellationError,
            ));
        }
        if max_depth == 0 || max_depth > MAX_TESSELLATION_DEPTH {
            return Err(GeometryError::new(
                GeometryErrorCode::InvalidTessellationDepth,
            ));
        }
        if !(2..=MAX_TESSELLATION_POINTS).contains(&max_points) {
            return Err(GeometryError::new(
                GeometryErrorCode::InvalidTessellationPointLimit,
            ));
        }
        Ok(Self {
            max_chord_error_m,
            max_depth,
            max_points,
        })
    }

    #[must_use]
    pub const fn max_chord_error_m(self) -> f64 {
        self.max_chord_error_m
    }

    #[must_use]
    pub const fn max_depth(self) -> u32 {
        self.max_depth
    }

    #[must_use]
    pub const fn max_points(self) -> u32 {
        self.max_points
    }
}

/// Tessellates a cubic in stable left-to-right order with bounded memory/work.
pub fn tessellate_cubic(
    curve: CubicBezier2,
    options: TessellationOptions,
    context: GeometryContext,
) -> Result<Vec<Point2Meters>, GeometryError> {
    let mut output = Vec::new();
    output.push(curve.start());
    let mut pending = vec![(curve, 0_u32)];
    while let Some((candidate, depth)) = pending.pop() {
        if cubic_flatness(candidate, context)? <= options.max_chord_error_m() {
            if output.len() >= options.max_points() as usize {
                return Err(GeometryError::new(
                    GeometryErrorCode::TessellationLimitExceeded,
                ));
            }
            output.push(candidate.end());
            continue;
        }
        if depth >= options.max_depth() {
            return Err(GeometryError::new(
                GeometryErrorCode::TessellationLimitExceeded,
            ));
        }
        let (left, right) = split_cubic(candidate)?;
        pending.push((right, depth + 1));
        pending.push((left, depth + 1));
    }
    Ok(output)
}

fn cubic_flatness(curve: CubicBezier2, context: GeometryContext) -> Result<f64, GeometryError> {
    let start = coordinates(curve.start());
    let end = coordinates(curve.end());
    let chord = checked_pair(subtract(end, start))?;
    let chord_length = checked_length(chord)?;
    if chord_length <= context.distance_tolerance_m() {
        return Ok(checked_length(checked_pair(subtract(
            coordinates(curve.control_1()),
            start,
        ))?)?
        .max(checked_length(checked_pair(subtract(
            coordinates(curve.control_2()),
            start,
        ))?)?));
    }
    let first_distance = checked_scalar(cross(
        checked_pair(subtract(coordinates(curve.control_1()), start))?,
        chord,
    ))?
    .abs()
        / chord_length;
    let second_distance = checked_scalar(cross(
        checked_pair(subtract(coordinates(curve.control_2()), start))?,
        chord,
    ))?
    .abs()
        / chord_length;
    checked_scalar(first_distance.max(second_distance))
}

fn split_cubic(curve: CubicBezier2) -> Result<(CubicBezier2, CubicBezier2), GeometryError> {
    let start = coordinates(curve.start());
    let control_1 = coordinates(curve.control_1());
    let control_2 = coordinates(curve.control_2());
    let end = coordinates(curve.end());
    let first = interpolate(start, control_1, 0.5)?;
    let second = interpolate(control_1, control_2, 0.5)?;
    let third = interpolate(control_2, end, 0.5)?;
    let fourth = interpolate(first, second, 0.5)?;
    let fifth = interpolate(second, third, 0.5)?;
    let midpoint = interpolate(fourth, fifth, 0.5)?;
    Ok((
        CubicBezier2::new(
            curve.start(),
            result_point(first)?,
            result_point(fourth)?,
            result_point(midpoint)?,
        ),
        CubicBezier2::new(
            result_point(midpoint)?,
            result_point(fifth)?,
            result_point(third)?,
            curve.end(),
        ),
    ))
}

fn interpolate(
    start: (f64, f64),
    end: (f64, f64),
    parameter: f64,
) -> Result<(f64, f64), GeometryError> {
    checked_pair((
        start.0 * (1.0 - parameter) + end.0 * parameter,
        start.1 * (1.0 - parameter) + end.1 * parameter,
    ))
}

/// A geometry failure with a stable code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryError(GeometryErrorCode);

impl GeometryError {
    const fn new(code: GeometryErrorCode) -> Self {
        Self(code)
    }

    #[must_use]
    pub const fn code(self) -> GeometryErrorCode {
        self.0
    }
}

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl Error for GeometryError {}

/// Caller-owned numerical policy for one related set of geometry operations.
///
/// Distance is in metres, orientation/cross-product tolerance is in square
/// metres, and parameter tolerance is dimensionless. Transition integration is
/// bounded by both a maximum panel length and an even panel count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryContext {
    distance_tolerance_m: f64,
    area_tolerance_m2: f64,
    parameter_tolerance: f64,
    max_integration_step_m: f64,
    max_integration_panels: u32,
}

impl GeometryContext {
    pub fn new(
        distance_tolerance_m: f64,
        area_tolerance_m2: f64,
        parameter_tolerance: f64,
        max_integration_step_m: f64,
        max_integration_panels: u32,
    ) -> Result<Self, GeometryError> {
        validate_non_negative(
            distance_tolerance_m,
            GeometryErrorCode::InvalidDistanceTolerance,
        )?;
        validate_non_negative(area_tolerance_m2, GeometryErrorCode::InvalidAreaTolerance)?;
        validate_non_negative(
            parameter_tolerance,
            GeometryErrorCode::InvalidParameterTolerance,
        )?;
        if !max_integration_step_m.is_finite() || max_integration_step_m <= 0.0 {
            return Err(GeometryError::new(
                GeometryErrorCode::InvalidIntegrationStep,
            ));
        }
        if !(2..=MAX_INTEGRATION_PANELS).contains(&max_integration_panels)
            || !max_integration_panels.is_multiple_of(2)
        {
            return Err(GeometryError::new(
                GeometryErrorCode::InvalidIntegrationPanelLimit,
            ));
        }
        Ok(Self {
            distance_tolerance_m,
            area_tolerance_m2,
            parameter_tolerance,
            max_integration_step_m,
            max_integration_panels,
        })
    }

    #[must_use]
    pub const fn distance_tolerance_m(self) -> f64 {
        self.distance_tolerance_m
    }

    #[must_use]
    pub const fn area_tolerance_m2(self) -> f64 {
        self.area_tolerance_m2
    }

    #[must_use]
    pub const fn parameter_tolerance(self) -> f64 {
        self.parameter_tolerance
    }

    #[must_use]
    pub const fn max_integration_step_m(self) -> f64 {
        self.max_integration_step_m
    }

    #[must_use]
    pub const fn max_integration_panels(self) -> u32 {
        self.max_integration_panels
    }
}

fn validate_non_negative(value: f64, code: GeometryErrorCode) -> Result<(), GeometryError> {
    if !value.is_finite() || value < 0.0 {
        return Err(GeometryError::new(code));
    }
    Ok(())
}

/// Position, tangent heading and signed curvature at one station.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveSample {
    position: Point2Meters,
    heading: HeadingRadians,
    curvature_per_m: CurvaturePerMeter,
}

impl CurveSample {
    #[must_use]
    pub const fn position(self) -> Point2Meters {
        self.position
    }

    #[must_use]
    pub const fn heading(self) -> HeadingRadians {
        self.heading
    }

    #[must_use]
    pub const fn curvature(self) -> CurvaturePerMeter {
        self.curvature_per_m
    }
}

#[derive(Clone, Copy)]
struct RawState {
    x_m: f64,
    y_m: f64,
    heading_rad: f64,
}

/// Evaluates a reference line at a closed-range station `[0, total_length]`.
///
/// An internal element boundary uses the outgoing element's curvature, matching
/// the half-open station ranges in the Design Model.
pub fn evaluate_reference_line(
    reference_line: &ReferenceLine,
    station: LengthMeters,
    context: GeometryContext,
) -> Result<CurveSample, GeometryError> {
    let target = station.get();
    if target > reference_line.total_length().get() {
        return Err(GeometryError::new(GeometryErrorCode::StationOutsideCurve));
    }
    let start = reference_line.start();
    let mut state = RawState {
        x_m: start.position().x_m().get(),
        y_m: start.position().y_m().get(),
        heading_rad: start.heading().get(),
    };
    let mut element_start = 0.0;
    let last_index = reference_line.segments().len() - 1;
    for (index, element) in reference_line.segments().iter().copied().enumerate() {
        let element_end = element_start + element.length().get();
        if target < element_end || index == last_index {
            let local_station = target - element_start;
            let (sample_state, curvature) =
                advance_element(state, element, local_station, context)?;
            return curve_sample(sample_state, curvature);
        }
        (state, _) = advance_element(state, element, element.length().get(), context)?;
        element_start = element_end;
    }
    unreachable!("ReferenceLine is non-empty and station is inside its total length")
}

fn advance_element(
    state: RawState,
    element: ReferenceLineElement,
    distance_m: f64,
    context: GeometryContext,
) -> Result<(RawState, f64), GeometryError> {
    match element {
        ReferenceLineElement::Line(_) => {
            let result = RawState {
                x_m: state.x_m + libm::cos(state.heading_rad) * distance_m,
                y_m: state.y_m + libm::sin(state.heading_rad) * distance_m,
                heading_rad: state.heading_rad,
            };
            ensure_finite_state(result)?;
            Ok((result, 0.0))
        }
        ReferenceLineElement::CircularArc(arc) => {
            let curvature = arc.curvature().get();
            let heading_change = curvature * distance_m;
            let end_heading = state.heading_rad + heading_change;
            let half_change = 0.5 * heading_change;
            let chord_length = distance_m * sinc(half_change);
            let chord_heading = state.heading_rad + half_change;
            let result = RawState {
                x_m: state.x_m + chord_length * libm::cos(chord_heading),
                y_m: state.y_m + chord_length * libm::sin(chord_heading),
                heading_rad: end_heading,
            };
            ensure_finite_state(result)?;
            Ok((result, curvature))
        }
        ReferenceLineElement::LinearCurvatureTransition(transition) => {
            let start_curvature = transition.start_curvature().get();
            let curvature_rate =
                (transition.end_curvature().get() - start_curvature) / transition.length().get();
            let end_heading = state.heading_rad
                + start_curvature * distance_m
                + 0.5 * curvature_rate * distance_m * distance_m;
            let (dx, dy) = integrate_transition(
                state.heading_rad,
                start_curvature,
                curvature_rate,
                distance_m,
                context,
            )?;
            let result = RawState {
                x_m: state.x_m + dx,
                y_m: state.y_m + dy,
                heading_rad: end_heading,
            };
            ensure_finite_state(result)?;
            Ok((result, start_curvature + curvature_rate * distance_m))
        }
    }
}

fn sinc(value: f64) -> f64 {
    if value == 0.0 {
        1.0
    } else {
        libm::sin(value) / value
    }
}

fn integrate_transition(
    heading_rad: f64,
    start_curvature: f64,
    curvature_rate: f64,
    distance_m: f64,
    context: GeometryContext,
) -> Result<(f64, f64), GeometryError> {
    if distance_m == 0.0 {
        return Ok((0.0, 0.0));
    }
    let required = libm::ceil(distance_m / context.max_integration_step_m());
    if !required.is_finite() || required > f64::from(context.max_integration_panels()) {
        return Err(GeometryError::new(
            GeometryErrorCode::IntegrationLimitExceeded,
        ));
    }
    let mut panels = (required as u32).max(2);
    if !panels.is_multiple_of(2) {
        panels += 1;
    }
    if panels > context.max_integration_panels() {
        return Err(GeometryError::new(
            GeometryErrorCode::IntegrationLimitExceeded,
        ));
    }
    let step = distance_m / f64::from(panels);
    let mut x_sum = 0.0;
    let mut y_sum = 0.0;
    for index in 0..=panels {
        let station = f64::from(index) * step;
        let angle =
            heading_rad + start_curvature * station + 0.5 * curvature_rate * station * station;
        let weight = if index == 0 || index == panels {
            1.0
        } else if index.is_multiple_of(2) {
            2.0
        } else {
            4.0
        };
        x_sum += weight * libm::cos(angle);
        y_sum += weight * libm::sin(angle);
    }
    let scale = step / 3.0;
    let result = (x_sum * scale, y_sum * scale);
    if !result.0.is_finite() || !result.1.is_finite() {
        return Err(GeometryError::new(GeometryErrorCode::NonFiniteResult));
    }
    Ok(result)
}

fn ensure_finite_state(state: RawState) -> Result<(), GeometryError> {
    if state.x_m.is_finite() && state.y_m.is_finite() && state.heading_rad.is_finite() {
        Ok(())
    } else {
        Err(GeometryError::new(GeometryErrorCode::NonFiniteResult))
    }
}

fn curve_sample(state: RawState, curvature: f64) -> Result<CurveSample, GeometryError> {
    ensure_finite_state(state)?;
    Ok(CurveSample {
        position: point(state.x_m, state.y_m)?,
        heading: HeadingRadians::try_new(state.heading_rad)
            .map_err(|_| GeometryError::new(GeometryErrorCode::NonFiniteResult))?,
        curvature_per_m: CurvaturePerMeter::try_new(curvature)
            .map_err(|_| GeometryError::new(GeometryErrorCode::NonFiniteResult))?,
    })
}

fn point(x_m: f64, y_m: f64) -> Result<Point2Meters, GeometryError> {
    Ok(Point2Meters::new(
        CoordinateMeters::try_new(x_m)
            .map_err(|_| GeometryError::new(GeometryErrorCode::NonFiniteResult))?,
        CoordinateMeters::try_new(y_m)
            .map_err(|_| GeometryError::new(GeometryErrorCode::NonFiniteResult))?,
    ))
}

/// Evaluates a signed left-positive parallel offset of a reference line.
pub fn evaluate_offset(
    reference_line: &ReferenceLine,
    station: LengthMeters,
    signed_offset_m: f64,
    context: GeometryContext,
) -> Result<CurveSample, GeometryError> {
    if !signed_offset_m.is_finite() {
        return Err(GeometryError::new(GeometryErrorCode::NonFiniteOffset));
    }
    let reference = evaluate_reference_line(reference_line, station, context)?;
    let scale = 1.0 - signed_offset_m * reference.curvature().get();
    if scale <= context.parameter_tolerance() {
        return Err(GeometryError::new(GeometryErrorCode::OffsetSingular));
    }
    let heading = reference.heading().get();
    let x_m = reference.position().x_m().get() - libm::sin(heading) * signed_offset_m;
    let y_m = reference.position().y_m().get() + libm::cos(heading) * signed_offset_m;
    curve_sample(
        RawState {
            x_m,
            y_m,
            heading_rad: heading,
        },
        reference.curvature().get() / scale,
    )
}

/// Derived lane center and both boundaries at one corridor station.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaneCrossSectionSample {
    lane_id: LaneId,
    width_m: LengthMeters,
    center: CurveSample,
    inner_boundary: CurveSample,
    outer_boundary: CurveSample,
    travel_heading: HeadingRadians,
}

impl LaneCrossSectionSample {
    #[must_use]
    pub const fn lane_id(self) -> LaneId {
        self.lane_id
    }

    #[must_use]
    pub const fn width(self) -> LengthMeters {
        self.width_m
    }

    #[must_use]
    pub const fn center(self) -> CurveSample {
        self.center
    }

    #[must_use]
    pub const fn inner_boundary(self) -> CurveSample {
        self.inner_boundary
    }

    #[must_use]
    pub const fn outer_boundary(self) -> CurveSample {
        self.outer_boundary
    }

    #[must_use]
    pub const fn travel_heading(self) -> HeadingRadians {
        self.travel_heading
    }
}

/// Evaluates the active piecewise-constant lane layout at one station.
pub fn evaluate_lane_cross_section(
    corridor: &Corridor,
    station: LengthMeters,
    context: GeometryContext,
) -> Result<Vec<LaneCrossSectionSample>, GeometryError> {
    if station.get() > corridor.reference_line().total_length().get() {
        return Err(GeometryError::new(GeometryErrorCode::StationOutsideCurve));
    }
    let section = corridor
        .cross_section_profile()
        .sections()
        .iter()
        .rfind(|section| section.start_station().get() <= station.get())
        .expect("Corridor profile starts at station zero");
    let mut result =
        Vec::with_capacity(section.layout().left().len() + section.layout().right().len());
    evaluate_side(
        corridor,
        station,
        section.layout().left(),
        1.0,
        context,
        &mut result,
    )?;
    evaluate_side(
        corridor,
        station,
        section.layout().right(),
        -1.0,
        context,
        &mut result,
    )?;
    Ok(result)
}

/// Continuity classification for one lane at a cross-section profile boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneProfileContinuity {
    Continuous,
    OffsetJump,
    Starts,
    Ends,
}

/// Explicit evidence at one piecewise cross-section boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaneProfileBoundary {
    station: LengthMeters,
    lane_id: LaneId,
    incoming_center_offset_m: Option<f64>,
    outgoing_center_offset_m: Option<f64>,
    continuity: LaneProfileContinuity,
}

impl LaneProfileBoundary {
    #[must_use]
    pub const fn station(self) -> LengthMeters {
        self.station
    }

    #[must_use]
    pub const fn lane_id(self) -> LaneId {
        self.lane_id
    }

    #[must_use]
    pub const fn incoming_center_offset_m(self) -> Option<f64> {
        self.incoming_center_offset_m
    }

    #[must_use]
    pub const fn outgoing_center_offset_m(self) -> Option<f64> {
        self.outgoing_center_offset_m
    }

    #[must_use]
    pub const fn continuity(self) -> LaneProfileContinuity {
        self.continuity
    }
}

/// Classifies every lane at every authored profile boundary in stable ID order.
pub fn lane_profile_boundaries(
    corridor: &Corridor,
    context: GeometryContext,
) -> Result<Vec<LaneProfileBoundary>, GeometryError> {
    let mut result = Vec::new();
    for pair in corridor.cross_section_profile().sections().windows(2) {
        let station = pair[1].start_station();
        let incoming = layout_center_offsets(pair[0].layout());
        let outgoing = layout_center_offsets(pair[1].layout());
        let lane_ids: BTreeSet<_> = incoming.keys().chain(outgoing.keys()).copied().collect();
        for lane_id in lane_ids {
            let incoming_offset = incoming.get(&lane_id).copied();
            let outgoing_offset = outgoing.get(&lane_id).copied();
            for offset in incoming_offset.into_iter().chain(outgoing_offset) {
                evaluate_offset(corridor.reference_line(), station, offset, context)?;
            }
            let continuity = match (incoming_offset, outgoing_offset) {
                (Some(before), Some(after))
                    if (after - before).abs() <= context.distance_tolerance_m() =>
                {
                    LaneProfileContinuity::Continuous
                }
                (Some(_), Some(_)) => LaneProfileContinuity::OffsetJump,
                (None, Some(_)) => LaneProfileContinuity::Starts,
                (Some(_), None) => LaneProfileContinuity::Ends,
                (None, None) => unreachable!("lane ID came from at least one profile section"),
            };
            result.push(LaneProfileBoundary {
                station,
                lane_id,
                incoming_center_offset_m: incoming_offset,
                outgoing_center_offset_m: outgoing_offset,
                continuity,
            });
        }
    }
    Ok(result)
}

fn layout_center_offsets(layout: &CrossSectionLayout) -> BTreeMap<LaneId, f64> {
    let mut offsets = BTreeMap::new();
    collect_side_offsets(layout.left(), 1.0, &mut offsets);
    collect_side_offsets(layout.right(), -1.0, &mut offsets);
    offsets
}

fn collect_side_offsets(slices: &[LaneSlice], side_sign: f64, output: &mut BTreeMap<LaneId, f64>) {
    let mut outward_width = 0.0;
    for slice in slices {
        let center_offset = side_sign * (outward_width + slice.width().get() * 0.5);
        outward_width += slice.width().get();
        let replaced = output.insert(slice.lane_id(), center_offset);
        debug_assert!(
            replaced.is_none(),
            "CrossSectionLayout rejects duplicate lanes"
        );
    }
}

fn evaluate_side(
    corridor: &Corridor,
    station: LengthMeters,
    slices: &[LaneSlice],
    side_sign: f64,
    context: GeometryContext,
    output: &mut Vec<LaneCrossSectionSample>,
) -> Result<(), GeometryError> {
    let mut outward_width = 0.0;
    for slice in slices {
        let inner_offset = side_sign * outward_width;
        let center_offset = side_sign * (outward_width + slice.width().get() * 0.5);
        outward_width += slice.width().get();
        let outer_offset = side_sign * outward_width;
        let center = evaluate_offset(corridor.reference_line(), station, center_offset, context)?;
        let lane = corridor
            .lane(slice.lane_id())
            .expect("Corridor validates every profile lane reference");
        let travel_heading = match lane.direction() {
            LaneDirection::AlongReference => center.heading(),
            LaneDirection::AgainstReference => {
                HeadingRadians::try_new(center.heading().get() + std::f64::consts::PI)
                    .expect("finite heading plus pi remains finite")
            }
        };
        output.push(LaneCrossSectionSample {
            lane_id: slice.lane_id(),
            width_m: slice.width(),
            center,
            inner_boundary: evaluate_offset(
                corridor.reference_line(),
                station,
                inner_offset,
                context,
            )?,
            outer_boundary: evaluate_offset(
                corridor.reference_line(),
                station,
                outer_offset,
                context,
            )?,
            travel_heading,
        });
    }
    Ok(())
}

/// A finite local-metric line segment. Zero-length segments are accepted and
/// handled explicitly by intersection predicates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment2 {
    start: Point2Meters,
    end: Point2Meters,
}

impl Segment2 {
    #[must_use]
    pub const fn new(start: Point2Meters, end: Point2Meters) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn start(self) -> Point2Meters {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> Point2Meters {
        self.end
    }
}

/// Deterministic finite-segment intersection classification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentIntersection {
    None,
    Point(Point2Meters),
    Overlap(Segment2),
}

/// Intersects two finite segments using only caller-provided tolerances.
/// Arithmetic overflow is reported instead of being interpreted as topology.
pub fn segment_intersection(
    first: Segment2,
    second: Segment2,
    context: GeometryContext,
) -> Result<SegmentIntersection, GeometryError> {
    let p = coordinates(first.start());
    let p2 = coordinates(first.end());
    let q = coordinates(second.start());
    let q2 = coordinates(second.end());
    let r = checked_pair(subtract(p2, p))?;
    let s = checked_pair(subtract(q2, q))?;
    let first_length = checked_length(r)?;
    let second_length = checked_length(s)?;
    let distance_tolerance = context.distance_tolerance_m();
    if first_length <= distance_tolerance && second_length <= distance_tolerance {
        return if checked_length(checked_pair(subtract(p, q))?)? <= distance_tolerance {
            Ok(SegmentIntersection::Point(result_point(midpoint(p, q))?))
        } else {
            Ok(SegmentIntersection::None)
        };
    }
    if first_length <= distance_tolerance {
        return degenerate_intersection(p, q, q2, context);
    }
    if second_length <= distance_tolerance {
        return degenerate_intersection(q, p, p2, context);
    }

    let q_minus_p = checked_pair(subtract(q, p))?;
    let r_cross_s = checked_scalar(cross(r, s))?;
    if r_cross_s.abs() <= context.area_tolerance_m2() {
        if checked_scalar(cross(q_minus_p, r))?.abs() > context.area_tolerance_m2()
            || checked_scalar(cross(q_minus_p, s))?.abs() > context.area_tolerance_m2()
        {
            return Ok(SegmentIntersection::None);
        }
        return collinear_intersection(p, r, q, q2, context.parameter_tolerance());
    }

    let t = checked_scalar(cross(q_minus_p, s))? / r_cross_s;
    let u = checked_scalar(cross(q_minus_p, r))? / r_cross_s;
    let tolerance = context.parameter_tolerance();
    if !inside_unit(t, tolerance) || !inside_unit(u, tolerance) {
        return Ok(SegmentIntersection::None);
    }
    let on_first = checked_pair(add(p, scale(r, t.clamp(0.0, 1.0))))?;
    let on_second = checked_pair(add(q, scale(s, u.clamp(0.0, 1.0))))?;
    Ok(SegmentIntersection::Point(result_point(midpoint(
        on_first, on_second,
    ))?))
}

fn degenerate_intersection(
    point_value: (f64, f64),
    segment_start: (f64, f64),
    segment_end: (f64, f64),
    context: GeometryContext,
) -> Result<SegmentIntersection, GeometryError> {
    let direction = checked_pair(subtract(segment_end, segment_start))?;
    let denominator = checked_scalar(dot(direction, direction))?;
    let parameter = checked_scalar(dot(
        checked_pair(subtract(point_value, segment_start))?,
        direction,
    ))? / denominator;
    if !inside_unit(parameter, context.parameter_tolerance()) {
        return Ok(SegmentIntersection::None);
    }
    let closest = checked_pair(add(
        segment_start,
        scale(direction, parameter.clamp(0.0, 1.0)),
    ))?;
    if checked_length(checked_pair(subtract(point_value, closest))?)?
        <= context.distance_tolerance_m()
    {
        Ok(SegmentIntersection::Point(result_point(midpoint(
            point_value,
            closest,
        ))?))
    } else {
        Ok(SegmentIntersection::None)
    }
}

fn collinear_intersection(
    p: (f64, f64),
    r: (f64, f64),
    q: (f64, f64),
    q2: (f64, f64),
    parameter_tolerance: f64,
) -> Result<SegmentIntersection, GeometryError> {
    let denominator = checked_scalar(dot(r, r))?;
    let first = checked_scalar(dot(checked_pair(subtract(q, p))?, r))? / denominator;
    let second = checked_scalar(dot(checked_pair(subtract(q2, p))?, r))? / denominator;
    let overlap_start = first.min(second).max(0.0);
    let overlap_end = first.max(second).min(1.0);
    if overlap_end < overlap_start - parameter_tolerance {
        return Ok(SegmentIntersection::None);
    }
    let start = checked_pair(add(p, scale(r, overlap_start.clamp(0.0, 1.0))))?;
    let end = checked_pair(add(p, scale(r, overlap_end.clamp(0.0, 1.0))))?;
    if checked_length(checked_pair(subtract(end, start))?)?
        <= parameter_tolerance * checked_length(r)?
    {
        return Ok(SegmentIntersection::Point(result_point(midpoint(
            start, end,
        ))?));
    }
    let (start, end) = canonical_pair(start, end);
    Ok(SegmentIntersection::Overlap(Segment2::new(
        result_point(start)?,
        result_point(end)?,
    )))
}

fn coordinates(value: Point2Meters) -> (f64, f64) {
    (value.x_m().get(), value.y_m().get())
}

fn result_point(value: (f64, f64)) -> Result<Point2Meters, GeometryError> {
    point(value.0, value.1)
}

const fn subtract(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 - right.0, left.1 - right.1)
}

const fn add(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 + right.0, left.1 + right.1)
}

const fn scale(value: (f64, f64), multiplier: f64) -> (f64, f64) {
    (value.0 * multiplier, value.1 * multiplier)
}

const fn dot(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.0 + left.1 * right.1
}

const fn cross(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.1 - left.1 * right.0
}

fn checked_length(value: (f64, f64)) -> Result<f64, GeometryError> {
    checked_scalar(libm::hypot(value.0, value.1))
}

const fn midpoint(first: (f64, f64), second: (f64, f64)) -> (f64, f64) {
    (
        first.0 * 0.5 + second.0 * 0.5,
        first.1 * 0.5 + second.1 * 0.5,
    )
}

fn inside_unit(value: f64, tolerance: f64) -> bool {
    value >= -tolerance && value <= 1.0 + tolerance
}

fn canonical_pair(first: (f64, f64), second: (f64, f64)) -> ((f64, f64), (f64, f64)) {
    if first.0.total_cmp(&second.0).is_lt()
        || (first.0 == second.0 && !first.1.total_cmp(&second.1).is_gt())
    {
        (first, second)
    } else {
        (second, first)
    }
}

fn checked_pair(value: (f64, f64)) -> Result<(f64, f64), GeometryError> {
    if value.0.is_finite() && value.1.is_finite() {
        Ok(value)
    } else {
        Err(GeometryError::new(GeometryErrorCode::NonFiniteResult))
    }
}

fn checked_scalar(value: f64) -> Result<f64, GeometryError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(GeometryError::new(GeometryErrorCode::NonFiniteResult))
    }
}
