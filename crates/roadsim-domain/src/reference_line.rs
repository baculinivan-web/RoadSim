use roadsim_types::{
    AngleRadians, CoordinateMeters, CurvaturePerMeter, CurvatureTolerancePerMeter, HeadingRadians,
    LengthMeters,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{error::Error, fmt};

/// Stable failure classifications for reference-line construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceLineErrorCode {
    Empty,
    ZeroLength,
    ZeroArcCurvature,
    ConstantTransitionCurvature,
    AngularChangeNonFinite,
    StationOverflow,
    StationResolutionLost,
}

impl ReferenceLineErrorCode {
    /// Returns the stable machine-readable diagnostic code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "domain.reference_line.empty",
            Self::ZeroLength => "domain.reference_line.segment.zero_length",
            Self::ZeroArcCurvature => "domain.reference_line.arc.zero_curvature",
            Self::ConstantTransitionCurvature => {
                "domain.reference_line.transition.constant_curvature"
            }
            Self::AngularChangeNonFinite => {
                "domain.reference_line.segment.angular_change_non_finite"
            }
            Self::StationOverflow => "domain.reference_line.station_overflow",
            Self::StationResolutionLost => "domain.reference_line.station_resolution_lost",
        }
    }
}

/// A reference-line validation failure with an optional ordered segment index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceLineError {
    code: ReferenceLineErrorCode,
    segment_index: Option<usize>,
}

impl ReferenceLineError {
    const fn global(code: ReferenceLineErrorCode) -> Self {
        Self {
            code,
            segment_index: None,
        }
    }

    const fn segment(code: ReferenceLineErrorCode, segment_index: usize) -> Self {
        Self {
            code,
            segment_index: Some(segment_index),
        }
    }

    /// Returns the stable failure classification.
    #[must_use]
    pub const fn code(self) -> ReferenceLineErrorCode {
        self.code
    }

    /// Returns the zero-based segment index when the failure is segment-specific.
    #[must_use]
    pub const fn segment_index(self) -> Option<usize> {
        self.segment_index
    }
}

impl fmt::Display for ReferenceLineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.segment_index {
            Some(index) => write!(formatter, "{} at segment {index}", self.code.as_str()),
            None => formatter.write_str(self.code.as_str()),
        }
    }
}

impl Error for ReferenceLineError {}

/// A point in the project's local metric engineering coordinate frame.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Point2Meters {
    x_m: CoordinateMeters,
    y_m: CoordinateMeters,
}

impl Point2Meters {
    #[must_use]
    pub const fn new(x_m: CoordinateMeters, y_m: CoordinateMeters) -> Self {
        Self { x_m, y_m }
    }

    #[must_use]
    pub const fn x_m(self) -> CoordinateMeters {
        self.x_m
    }

    #[must_use]
    pub const fn y_m(self) -> CoordinateMeters {
        self.y_m
    }
}

/// The single explicitly stored start pose of a compositional reference line.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReferenceLinePose {
    position: Point2Meters,
    heading: HeadingRadians,
}

impl ReferenceLinePose {
    #[must_use]
    pub const fn new(position: Point2Meters, heading: HeadingRadians) -> Self {
        Self { position, heading }
    }

    #[must_use]
    pub const fn position(self) -> Point2Meters {
        self.position
    }

    #[must_use]
    pub const fn heading(self) -> HeadingRadians {
        self.heading
    }
}

fn validate_positive_length(length: LengthMeters) -> Result<(), ReferenceLineError> {
    if length.get() == 0.0 {
        return Err(ReferenceLineError::global(
            ReferenceLineErrorCode::ZeroLength,
        ));
    }
    Ok(())
}

/// A zero-curvature line element.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LineSegment {
    length_m: LengthMeters,
}

impl LineSegment {
    pub fn new(length_m: LengthMeters) -> Result<Self, ReferenceLineError> {
        validate_positive_length(length_m)?;
        Ok(Self { length_m })
    }

    #[must_use]
    pub const fn length(self) -> LengthMeters {
        self.length_m
    }
}

impl<'de> Deserialize<'de> for LineSegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            length_m: LengthMeters,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.length_m).map_err(serde::de::Error::custom)
    }
}

/// A constant, non-zero signed-curvature circular arc.
///
/// Positive curvature turns left along increasing station; negative curvature
/// turns right.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CircularArcSegment {
    length_m: LengthMeters,
    curvature_per_m: CurvaturePerMeter,
}

impl CircularArcSegment {
    pub fn new(
        length_m: LengthMeters,
        curvature_per_m: CurvaturePerMeter,
    ) -> Result<Self, ReferenceLineError> {
        validate_positive_length(length_m)?;
        if curvature_per_m.get() == 0.0 {
            return Err(ReferenceLineError::global(
                ReferenceLineErrorCode::ZeroArcCurvature,
            ));
        }
        validate_angular_change(length_m.get() * curvature_per_m.get())?;
        Ok(Self {
            length_m,
            curvature_per_m,
        })
    }

    #[must_use]
    pub const fn length(self) -> LengthMeters {
        self.length_m
    }

    #[must_use]
    pub const fn curvature(self) -> CurvaturePerMeter {
        self.curvature_per_m
    }
}

impl<'de> Deserialize<'de> for CircularArcSegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            length_m: LengthMeters,
            curvature_per_m: CurvaturePerMeter,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.length_m, wire.curvature_per_m).map_err(serde::de::Error::custom)
    }
}

/// A transition whose signed curvature varies linearly with arc-length station.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LinearCurvatureTransition {
    length_m: LengthMeters,
    start_curvature_per_m: CurvaturePerMeter,
    end_curvature_per_m: CurvaturePerMeter,
}

impl LinearCurvatureTransition {
    pub fn new(
        length_m: LengthMeters,
        start_curvature_per_m: CurvaturePerMeter,
        end_curvature_per_m: CurvaturePerMeter,
    ) -> Result<Self, ReferenceLineError> {
        validate_positive_length(length_m)?;
        if start_curvature_per_m == end_curvature_per_m {
            return Err(ReferenceLineError::global(
                ReferenceLineErrorCode::ConstantTransitionCurvature,
            ));
        }
        let mean_curvature = 0.5 * start_curvature_per_m.get() + 0.5 * end_curvature_per_m.get();
        validate_angular_change(length_m.get() * mean_curvature)?;
        Ok(Self {
            length_m,
            start_curvature_per_m,
            end_curvature_per_m,
        })
    }

    #[must_use]
    pub const fn length(self) -> LengthMeters {
        self.length_m
    }

    #[must_use]
    pub const fn start_curvature(self) -> CurvaturePerMeter {
        self.start_curvature_per_m
    }

    #[must_use]
    pub const fn end_curvature(self) -> CurvaturePerMeter {
        self.end_curvature_per_m
    }
}

impl<'de> Deserialize<'de> for LinearCurvatureTransition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            length_m: LengthMeters,
            start_curvature_per_m: CurvaturePerMeter,
            end_curvature_per_m: CurvaturePerMeter,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.length_m,
            wire.start_curvature_per_m,
            wire.end_curvature_per_m,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn validate_angular_change(value: f64) -> Result<(), ReferenceLineError> {
    if !value.is_finite() {
        return Err(ReferenceLineError::global(
            ReferenceLineErrorCode::AngularChangeNonFinite,
        ));
    }
    Ok(())
}

/// The stable semantic kind of a reference-line element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceLineElementKind {
    Line,
    CircularArc,
    LinearCurvatureTransition,
}

/// One ordered parametric element of a reference line.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum ReferenceLineElement {
    Line(LineSegment),
    CircularArc(CircularArcSegment),
    LinearCurvatureTransition(LinearCurvatureTransition),
}

impl ReferenceLineElement {
    pub fn line(length_m: LengthMeters) -> Result<Self, ReferenceLineError> {
        LineSegment::new(length_m).map(Self::Line)
    }

    pub fn circular_arc(
        length_m: LengthMeters,
        curvature_per_m: CurvaturePerMeter,
    ) -> Result<Self, ReferenceLineError> {
        CircularArcSegment::new(length_m, curvature_per_m).map(Self::CircularArc)
    }

    pub fn linear_curvature_transition(
        length_m: LengthMeters,
        start_curvature_per_m: CurvaturePerMeter,
        end_curvature_per_m: CurvaturePerMeter,
    ) -> Result<Self, ReferenceLineError> {
        LinearCurvatureTransition::new(length_m, start_curvature_per_m, end_curvature_per_m)
            .map(Self::LinearCurvatureTransition)
    }

    #[must_use]
    pub const fn kind(self) -> ReferenceLineElementKind {
        match self {
            Self::Line(_) => ReferenceLineElementKind::Line,
            Self::CircularArc(_) => ReferenceLineElementKind::CircularArc,
            Self::LinearCurvatureTransition(_) => {
                ReferenceLineElementKind::LinearCurvatureTransition
            }
        }
    }

    #[must_use]
    pub const fn length(self) -> LengthMeters {
        match self {
            Self::Line(value) => value.length(),
            Self::CircularArc(value) => value.length(),
            Self::LinearCurvatureTransition(value) => value.length(),
        }
    }

    #[must_use]
    pub fn heading_change(self) -> AngleRadians {
        AngleRadians::try_new(self.heading_change_radians())
            .expect("constructors guarantee a finite angular change")
    }

    #[must_use]
    pub fn start_curvature(self) -> CurvaturePerMeter {
        match self {
            Self::Line(_) => CurvaturePerMeter::try_new(0.0).expect("zero is a finite curvature"),
            Self::CircularArc(value) => value.curvature(),
            Self::LinearCurvatureTransition(value) => value.start_curvature(),
        }
    }

    #[must_use]
    pub fn end_curvature(self) -> CurvaturePerMeter {
        match self {
            Self::Line(_) => CurvaturePerMeter::try_new(0.0).expect("zero is a finite curvature"),
            Self::CircularArc(value) => value.curvature(),
            Self::LinearCurvatureTransition(value) => value.end_curvature(),
        }
    }

    fn heading_change_radians(self) -> f64 {
        match self {
            Self::Line(_) => 0.0,
            Self::CircularArc(value) => value.length().get() * value.curvature().get(),
            Self::LinearCurvatureTransition(value) => {
                let mean_curvature =
                    0.5 * value.start_curvature().get() + 0.5 * value.end_curvature().get();
                value.length().get() * mean_curvature
            }
        }
    }
}

/// A half-open semantic station interval for one ordered element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StationRange {
    start: LengthMeters,
    end: LengthMeters,
}

impl StationRange {
    pub(crate) fn new(start: LengthMeters, end: LengthMeters) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn start(self) -> LengthMeters {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> LengthMeters {
        self.end
    }
}

/// Explicit continuity classification at an element boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryContinuity {
    /// Position and tangent are continuous by construction; curvature jumps.
    PositionAndTangent,
    /// Position, tangent and curvature are continuous within caller tolerance.
    PositionTangentAndCurvature,
}

/// Derived evidence for one adjacent pair of reference-line elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceLineBoundary {
    station: LengthMeters,
    heading: HeadingRadians,
    incoming_curvature: CurvaturePerMeter,
    outgoing_curvature: CurvaturePerMeter,
}

impl ReferenceLineBoundary {
    #[must_use]
    pub const fn station(self) -> LengthMeters {
        self.station
    }

    #[must_use]
    pub const fn heading(self) -> HeadingRadians {
        self.heading
    }

    #[must_use]
    pub const fn incoming_curvature(self) -> CurvaturePerMeter {
        self.incoming_curvature
    }

    #[must_use]
    pub const fn outgoing_curvature(self) -> CurvaturePerMeter {
        self.outgoing_curvature
    }

    /// Classifies curvature continuity using an operation-supplied tolerance.
    #[must_use]
    pub fn continuity(self, tolerance: CurvatureTolerancePerMeter) -> BoundaryContinuity {
        let gap = (self.outgoing_curvature.get() - self.incoming_curvature.get()).abs();
        if gap <= tolerance.get() {
            BoundaryContinuity::PositionTangentAndCurvature
        } else {
            BoundaryContinuity::PositionAndTangent
        }
    }
}

/// A non-empty ordered reference line in the Design Model.
///
/// Only the first pose and semantic element parameters are stored. Element start
/// stations, headings and boundary evidence are derived in stable vector order.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReferenceLine {
    start: ReferenceLinePose,
    segments: Vec<ReferenceLineElement>,
    #[serde(skip)]
    total_length: LengthMeters,
}

impl ReferenceLine {
    pub fn new(
        start: ReferenceLinePose,
        segments: Vec<ReferenceLineElement>,
    ) -> Result<Self, ReferenceLineError> {
        if segments.is_empty() {
            return Err(ReferenceLineError::global(ReferenceLineErrorCode::Empty));
        }

        let mut total = 0.0;
        for (index, segment) in segments.iter().copied().enumerate() {
            let next = total + segment.length().get();
            if !next.is_finite() {
                return Err(ReferenceLineError::segment(
                    ReferenceLineErrorCode::StationOverflow,
                    index,
                ));
            }
            if next <= total {
                return Err(ReferenceLineError::segment(
                    ReferenceLineErrorCode::StationResolutionLost,
                    index,
                ));
            }
            total = next;
        }
        let total_length = LengthMeters::try_new(total)
            .expect("positive finite segment accumulation was validated");

        Ok(Self {
            start,
            segments,
            total_length,
        })
    }

    #[must_use]
    pub const fn start(&self) -> ReferenceLinePose {
        self.start
    }

    #[must_use]
    pub fn segments(&self) -> &[ReferenceLineElement] {
        &self.segments
    }

    #[must_use]
    pub const fn total_length(&self) -> LengthMeters {
        self.total_length
    }

    /// Derives contiguous ranges in serialized segment order.
    #[must_use]
    pub fn station_ranges(&self) -> Vec<StationRange> {
        let mut station = 0.0;
        self.segments
            .iter()
            .map(|segment| {
                let start = station;
                station += segment.length().get();
                StationRange {
                    start: LengthMeters::try_new(start).expect("validated station is finite"),
                    end: LengthMeters::try_new(station).expect("validated station is finite"),
                }
            })
            .collect()
    }

    /// Derives one boundary record for every adjacent element pair.
    #[must_use]
    pub fn boundaries(&self) -> Vec<ReferenceLineBoundary> {
        let mut station = 0.0;
        let mut heading = self.start.heading();
        let mut result = Vec::with_capacity(self.segments.len().saturating_sub(1));

        for pair in self.segments.windows(2) {
            let incoming = pair[0];
            station += incoming.length().get();
            heading = HeadingRadians::try_new(heading.get() + incoming.heading_change_radians())
                .expect("validated angular change preserves a finite heading");
            result.push(ReferenceLineBoundary {
                station: LengthMeters::try_new(station).expect("validated station is finite"),
                heading,
                incoming_curvature: incoming.end_curvature(),
                outgoing_curvature: pair[1].start_curvature(),
            });
        }
        result
    }
}

impl<'de> Deserialize<'de> for ReferenceLine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            start: ReferenceLinePose,
            segments: Vec<ReferenceLineElement>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start, wire.segments).map_err(serde::de::Error::custom)
    }
}
