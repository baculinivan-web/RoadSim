use proptest::prelude::*;
use roadsim_domain::{
    BoundaryContinuity, Point2Meters, ReferenceLine, ReferenceLineElement, ReferenceLineErrorCode,
    ReferenceLinePose,
};
use roadsim_types::{
    CoordinateMeters, CurvaturePerMeter, CurvatureTolerancePerMeter, HeadingRadians, LengthMeters,
    ValueErrorCode,
};

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}

fn curvature(value: f64) -> CurvaturePerMeter {
    CurvaturePerMeter::try_new(value).unwrap()
}

fn start_pose(heading: f64) -> ReferenceLinePose {
    ReferenceLinePose::new(
        Point2Meters::new(
            CoordinateMeters::try_new(10.0).unwrap(),
            CoordinateMeters::try_new(-5.0).unwrap(),
        ),
        HeadingRadians::try_new(heading).unwrap(),
    )
}

#[test]
fn line_transition_arc_chain_reports_boundary_continuity() {
    let curve = ReferenceLine::new(
        start_pose(0.25),
        vec![
            ReferenceLineElement::line(length(20.0)).unwrap(),
            ReferenceLineElement::linear_curvature_transition(
                length(10.0),
                curvature(0.0),
                curvature(0.05),
            )
            .unwrap(),
            ReferenceLineElement::circular_arc(length(15.0), curvature(0.05)).unwrap(),
        ],
    )
    .unwrap();

    assert_eq!(curve.total_length().get(), 45.0);
    let ranges = curve.station_ranges();
    assert_eq!(ranges.len(), 3);
    assert_eq!(
        (ranges[0].start().get(), ranges[0].end().get()),
        (0.0, 20.0)
    );
    assert_eq!(
        (ranges[1].start().get(), ranges[1].end().get()),
        (20.0, 30.0)
    );
    assert_eq!(
        (ranges[2].start().get(), ranges[2].end().get()),
        (30.0, 45.0)
    );

    let boundaries = curve.boundaries();
    assert_eq!(boundaries.len(), 2);
    let exact = CurvatureTolerancePerMeter::try_new(0.0).unwrap();
    assert_eq!(boundaries[0].station().get(), 20.0);
    assert_eq!(boundaries[1].station().get(), 30.0);
    assert_eq!(
        boundaries[0].continuity(exact),
        BoundaryContinuity::PositionTangentAndCurvature
    );
    assert_eq!(
        boundaries[1].continuity(exact),
        BoundaryContinuity::PositionTangentAndCurvature
    );
    assert_eq!(boundaries[0].heading().get(), 0.25);
    assert!((boundaries[1].heading().get() - 0.5).abs() < 1.0e-14);
}

#[test]
fn direct_line_arc_joint_exposes_curvature_jump_with_caller_tolerance() {
    let curve = ReferenceLine::new(
        start_pose(0.0),
        vec![
            ReferenceLineElement::line(length(5.0)).unwrap(),
            ReferenceLineElement::circular_arc(length(5.0), curvature(-0.1)).unwrap(),
        ],
    )
    .unwrap();
    let boundary = curve.boundaries()[0];

    assert_eq!(boundary.incoming_curvature().get(), 0.0);
    assert_eq!(boundary.outgoing_curvature().get(), -0.1);
    assert_eq!(
        boundary.continuity(CurvatureTolerancePerMeter::try_new(0.099).unwrap()),
        BoundaryContinuity::PositionAndTangent
    );
    assert_eq!(
        boundary.continuity(CurvatureTolerancePerMeter::try_new(0.1).unwrap()),
        BoundaryContinuity::PositionTangentAndCurvature
    );
}

#[test]
fn signed_arc_curvature_controls_heading_direction() {
    let left = ReferenceLineElement::circular_arc(length(10.0), curvature(0.2)).unwrap();
    let right = ReferenceLineElement::circular_arc(length(10.0), curvature(-0.2)).unwrap();

    assert_eq!(left.heading_change().get(), 2.0);
    assert_eq!(right.heading_change().get(), -2.0);
}

#[test]
fn invalid_or_noncanonical_primitives_are_rejected() {
    assert_eq!(
        ReferenceLine::new(start_pose(0.0), vec![])
            .unwrap_err()
            .code(),
        ReferenceLineErrorCode::Empty
    );
    assert_eq!(
        ReferenceLineElement::line(length(0.0)).unwrap_err().code(),
        ReferenceLineErrorCode::ZeroLength
    );
    assert_eq!(
        ReferenceLineElement::circular_arc(length(1.0), curvature(0.0))
            .unwrap_err()
            .code(),
        ReferenceLineErrorCode::ZeroArcCurvature
    );
    assert_eq!(
        ReferenceLineElement::linear_curvature_transition(
            length(1.0),
            curvature(0.25),
            curvature(0.25),
        )
        .unwrap_err()
        .code(),
        ReferenceLineErrorCode::ConstantTransitionCurvature
    );
    assert_eq!(
        CurvaturePerMeter::try_new(f64::NAN).unwrap_err().code(),
        ValueErrorCode::NonFinite
    );
    assert_eq!(
        CurvatureTolerancePerMeter::try_new(-0.1)
            .unwrap_err()
            .code(),
        ValueErrorCode::Negative
    );
}

#[test]
fn derived_overflow_and_invalid_deserialization_are_rejected() {
    let huge = length(f64::MAX);
    let error = ReferenceLine::new(
        start_pose(0.0),
        vec![
            ReferenceLineElement::line(huge).unwrap(),
            ReferenceLineElement::line(huge).unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(error.code(), ReferenceLineErrorCode::StationOverflow);
    assert_eq!(error.segment_index(), Some(1));

    let resolution_loss = ReferenceLine::new(
        start_pose(0.0),
        vec![
            ReferenceLineElement::line(huge).unwrap(),
            ReferenceLineElement::line(length(1.0)).unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(
        resolution_loss.code(),
        ReferenceLineErrorCode::StationResolutionLost
    );
    assert_eq!(resolution_loss.segment_index(), Some(1));

    let angular_overflow = ReferenceLineElement::circular_arc(huge, curvature(2.0)).unwrap_err();
    assert_eq!(
        angular_overflow.code(),
        ReferenceLineErrorCode::AngularChangeNonFinite
    );

    let zero_line = r#"{"start":{"position":{"x_m":0.0,"y_m":0.0},"heading":0.0},"segments":[{"type":"line","data":{"length_m":0.0}}]}"#;
    assert!(serde_json::from_str::<ReferenceLine>(zero_line).is_err());
}

#[test]
fn reference_line_round_trip_preserves_order_and_semantics() {
    let curve = ReferenceLine::new(
        start_pose(-0.25),
        vec![
            ReferenceLineElement::line(length(2.0)).unwrap(),
            ReferenceLineElement::circular_arc(length(3.0), curvature(0.2)).unwrap(),
            ReferenceLineElement::linear_curvature_transition(
                length(4.0),
                curvature(0.2),
                curvature(-0.1),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let json = serde_json::to_string(&curve).unwrap();
    let restored: ReferenceLine = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, curve);
    assert_eq!(restored.segments(), curve.segments());
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn station_ranges_are_monotonic_and_contiguous(
        raw_lengths in prop::collection::vec(1.0e-9_f64..1.0e6, 1..64),
    ) {
        let segments = raw_lengths
            .iter()
            .copied()
            .map(|value| ReferenceLineElement::line(length(value)).unwrap())
            .collect();
        let curve = ReferenceLine::new(start_pose(0.0), segments).unwrap();
        let ranges = curve.station_ranges();

        prop_assert_eq!(ranges.len(), raw_lengths.len());
        prop_assert_eq!(ranges[0].start().get(), 0.0);
        for pair in ranges.windows(2) {
            prop_assert_eq!(pair[0].end(), pair[1].start());
            prop_assert!(pair[0].start().get() < pair[0].end().get());
        }
        prop_assert!(ranges.last().unwrap().start().get() < ranges.last().unwrap().end().get());
        prop_assert_eq!(ranges.last().unwrap().end(), curve.total_length());
    }

    #[test]
    fn matching_transition_boundaries_are_curvature_continuous(
        heading in -1000.0_f64..1000.0,
        line_length in 1.0e-6_f64..1000.0,
        transition_length in 1.0e-6_f64..1000.0,
        arc_length in 1.0e-6_f64..1000.0,
        end_curvature in prop_oneof![-1.0_f64..-1.0e-9, 1.0e-9_f64..1.0],
    ) {
        let curve = ReferenceLine::new(
            start_pose(heading),
            vec![
                ReferenceLineElement::line(length(line_length)).unwrap(),
                ReferenceLineElement::linear_curvature_transition(
                    length(transition_length),
                    curvature(0.0),
                    curvature(end_curvature),
                ).unwrap(),
                ReferenceLineElement::circular_arc(
                    length(arc_length),
                    curvature(end_curvature),
                ).unwrap(),
            ],
        ).unwrap();
        let exact = CurvatureTolerancePerMeter::try_new(0.0).unwrap();

        for boundary in curve.boundaries() {
            prop_assert_eq!(
                boundary.continuity(exact),
                BoundaryContinuity::PositionTangentAndCurvature,
            );
            prop_assert!(boundary.heading().get() >= 0.0);
            prop_assert!(boundary.heading().get() < std::f64::consts::TAU);
        }
    }
}
