use proptest::prelude::*;
use roadsim_domain::{
    Corridor, CrossSectionLayout, CrossSectionProfile, CrossSectionSection, LaneDefinition,
    LaneDirection, LaneSlice, LaneUse, LinearCurvatureTransition, Point2Meters, ReferenceLine,
    ReferenceLineElement, ReferenceLinePose,
};
use roadsim_geometry::{
    GeometryContext, GeometryErrorCode, LaneProfileContinuity, Segment2, SegmentIntersection,
    evaluate_lane_cross_section, evaluate_offset, evaluate_reference_line, lane_profile_boundaries,
    segment_intersection,
};
use roadsim_types::{
    CoordinateMeters, CorridorId, CurvaturePerMeter, HeadingRadians, LaneId, LengthMeters,
};

fn length(value: f64) -> LengthMeters {
    LengthMeters::try_new(value).unwrap()
}

fn curvature(value: f64) -> CurvaturePerMeter {
    CurvaturePerMeter::try_new(value).unwrap()
}

fn point(x_m: f64, y_m: f64) -> Point2Meters {
    Point2Meters::new(
        CoordinateMeters::try_new(x_m).unwrap(),
        CoordinateMeters::try_new(y_m).unwrap(),
    )
}

fn context() -> GeometryContext {
    GeometryContext::new(1.0e-9, 1.0e-12, 1.0e-12, 0.01, 100_000).unwrap()
}

fn reference_line(segments: Vec<ReferenceLineElement>) -> ReferenceLine {
    ReferenceLine::new(
        ReferenceLinePose::new(point(0.0, 0.0), HeadingRadians::try_new(0.0).unwrap()),
        segments,
    )
    .unwrap()
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

#[test]
fn geometry_context_rejects_invalid_or_unbounded_policy() {
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(
            GeometryContext::new(invalid, 0.0, 0.0, 1.0, 2)
                .unwrap_err()
                .code(),
            GeometryErrorCode::InvalidDistanceTolerance
        );
    }
    assert_eq!(
        GeometryContext::new(0.0, 0.0, 0.0, 0.0, 2)
            .unwrap_err()
            .code(),
        GeometryErrorCode::InvalidIntegrationStep
    );
    assert_eq!(
        GeometryContext::new(0.0, 0.0, 0.0, 1.0, 3)
            .unwrap_err()
            .code(),
        GeometryErrorCode::InvalidIntegrationPanelLimit
    );
}

#[test]
fn line_and_arc_use_analytic_station_evaluation() {
    let line = reference_line(vec![ReferenceLineElement::line(length(10.0)).unwrap()]);
    let line_end = evaluate_reference_line(&line, length(10.0), context()).unwrap();
    assert_eq!(line_end.position(), point(10.0, 0.0));
    assert_eq!(line_end.heading().get(), 0.0);
    assert_eq!(line_end.curvature().get(), 0.0);

    let radius = 10.0;
    let arc = reference_line(vec![
        ReferenceLineElement::circular_arc(
            length(std::f64::consts::FRAC_PI_2 * radius),
            curvature(1.0 / radius),
        )
        .unwrap(),
    ]);
    let arc_end = evaluate_reference_line(&arc, arc.total_length(), context()).unwrap();
    assert_close(arc_end.position().x_m().get(), radius, 1.0e-12);
    assert_close(arc_end.position().y_m().get(), radius, 1.0e-12);
    assert_close(
        arc_end.heading().get(),
        std::f64::consts::FRAC_PI_2,
        1.0e-12,
    );
}

#[test]
fn near_zero_arc_curvature_uses_the_stable_line_limit() {
    let arc = reference_line(vec![
        ReferenceLineElement::circular_arc(length(10.0), curvature(1.0e-300)).unwrap(),
    ]);
    let end = evaluate_reference_line(&arc, length(10.0), context()).unwrap();
    assert_close(end.position().x_m().get(), 10.0, 1.0e-12);
    assert!(end.position().y_m().get().abs() <= 1.0e-298);
}

#[test]
fn transition_evaluation_is_bounded_and_preserves_analytic_heading() {
    let transition = reference_line(vec![ReferenceLineElement::LinearCurvatureTransition(
        LinearCurvatureTransition::new(length(20.0), curvature(0.0), curvature(0.05)).unwrap(),
    )]);
    let sample = evaluate_reference_line(&transition, length(20.0), context()).unwrap();
    assert_close(sample.heading().get(), 0.5, 1.0e-12);
    assert_close(sample.curvature().get(), 0.05, 1.0e-12);
    assert!(sample.position().x_m().get().is_finite());
    assert!(sample.position().y_m().get().is_finite());

    let bounded = GeometryContext::new(0.0, 0.0, 0.0, 0.01, 100).unwrap();
    assert_eq!(
        evaluate_reference_line(&transition, length(20.0), bounded)
            .unwrap_err()
            .code(),
        GeometryErrorCode::IntegrationLimitExceeded
    );
}

#[test]
fn element_boundary_uses_outgoing_curvature_and_outside_station_fails() {
    let curve = reference_line(vec![
        ReferenceLineElement::line(length(5.0)).unwrap(),
        ReferenceLineElement::circular_arc(length(5.0), curvature(0.1)).unwrap(),
    ]);
    let boundary = evaluate_reference_line(&curve, length(5.0), context()).unwrap();
    assert_eq!(boundary.position(), point(5.0, 0.0));
    assert_eq!(boundary.curvature().get(), 0.1);
    assert_eq!(
        evaluate_reference_line(&curve, length(10.1), context())
            .unwrap_err()
            .code(),
        GeometryErrorCode::StationOutsideCurve
    );
}

#[test]
fn offset_detects_singularity_without_changing_reference_line() {
    let curve = reference_line(vec![
        ReferenceLineElement::circular_arc(length(5.0), curvature(0.1)).unwrap(),
    ]);
    let offset = evaluate_offset(&curve, length(0.0), 2.0, context()).unwrap();
    assert_eq!(offset.position(), point(0.0, 2.0));
    assert_close(offset.curvature().get(), 0.125, 1.0e-12);
    assert_eq!(
        evaluate_offset(&curve, length(0.0), 10.0, context())
            .unwrap_err()
            .code(),
        GeometryErrorCode::OffsetSingular
    );
}

#[test]
fn lane_cross_section_selects_profile_and_travel_orientation() {
    let left = LaneId::from_u128(10);
    let right = LaneId::from_u128(11);
    let corridor = Corridor::new(
        CorridorId::from_u128(1),
        reference_line(vec![ReferenceLineElement::line(length(20.0)).unwrap()]),
        vec![
            LaneDefinition::new(left, LaneDirection::AlongReference, LaneUse::GeneralTraffic),
            LaneDefinition::new(
                right,
                LaneDirection::AgainstReference,
                LaneUse::GeneralTraffic,
            ),
        ],
        CrossSectionProfile::new(vec![
            CrossSectionSection::new(
                length(0.0),
                CrossSectionLayout::new(
                    vec![LaneSlice::new(left, length(3.0)).unwrap()],
                    vec![LaneSlice::new(right, length(3.0)).unwrap()],
                )
                .unwrap(),
            ),
            CrossSectionSection::new(
                length(10.0),
                CrossSectionLayout::new(
                    vec![LaneSlice::new(left, length(4.0)).unwrap()],
                    vec![LaneSlice::new(right, length(4.0)).unwrap()],
                )
                .unwrap(),
            ),
        ])
        .unwrap(),
    )
    .unwrap();

    let samples = evaluate_lane_cross_section(&corridor, length(10.0), context()).unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].lane_id(), left);
    assert_eq!(samples[0].width().get(), 4.0);
    assert_eq!(samples[0].center().position(), point(10.0, 2.0));
    assert_eq!(samples[0].inner_boundary().position(), point(10.0, 0.0));
    assert_eq!(samples[0].outer_boundary().position(), point(10.0, 4.0));
    assert_close(samples[1].travel_heading().get(), std::f64::consts::PI, 0.0);

    let boundaries = lane_profile_boundaries(&corridor, context()).unwrap();
    assert_eq!(boundaries.len(), 2);
    assert_eq!(boundaries[0].station().get(), 10.0);
    assert_eq!(boundaries[0].lane_id(), left);
    assert_eq!(boundaries[0].incoming_center_offset_m(), Some(1.5));
    assert_eq!(boundaries[0].outgoing_center_offset_m(), Some(2.0));
    assert_eq!(
        boundaries[0].continuity(),
        LaneProfileContinuity::OffsetJump
    );
    assert_eq!(boundaries[1].lane_id(), right);
    assert_eq!(
        boundaries[1].continuity(),
        LaneProfileContinuity::OffsetJump
    );
}

#[test]
fn segment_intersections_cover_crossing_touch_overlap_and_degenerate() {
    let horizontal = Segment2::new(point(0.0, 0.0), point(10.0, 0.0));
    let vertical = Segment2::new(point(5.0, -5.0), point(5.0, 5.0));
    assert_eq!(
        segment_intersection(horizontal, vertical, context()).unwrap(),
        SegmentIntersection::Point(point(5.0, 0.0))
    );
    assert_eq!(
        segment_intersection(
            horizontal,
            Segment2::new(point(10.0, 0.0), point(15.0, 5.0)),
            context(),
        )
        .unwrap(),
        SegmentIntersection::Point(point(10.0, 0.0))
    );
    assert_eq!(
        segment_intersection(
            horizontal,
            Segment2::new(point(4.0, 0.0), point(12.0, 0.0)),
            context(),
        )
        .unwrap(),
        SegmentIntersection::Overlap(Segment2::new(point(4.0, 0.0), point(10.0, 0.0)))
    );
    assert_eq!(
        segment_intersection(
            Segment2::new(point(5.0, 0.0), point(5.0, 0.0)),
            horizontal,
            context(),
        )
        .unwrap(),
        SegmentIntersection::Point(point(5.0, 0.0))
    );
}

#[test]
fn predicate_overflow_is_an_error_instead_of_a_panic() {
    let enormous = f64::MAX;
    let error = segment_intersection(
        Segment2::new(point(enormous, 0.0), point(-enormous, 0.0)),
        Segment2::new(point(0.0, -1.0), point(0.0, 1.0)),
        context(),
    )
    .unwrap_err();
    assert_eq!(error.code(), GeometryErrorCode::NonFiniteResult);
}

proptest! {
    #[test]
    fn line_evaluation_is_finite_and_station_monotonic(
        length_m in 1.0_f64..10_000.0,
        fraction in 0.0_f64..=1.0,
        heading in 0.0_f64..std::f64::consts::TAU,
    ) {
        let curve = ReferenceLine::new(
            ReferenceLinePose::new(point(5.0, -3.0), HeadingRadians::try_new(heading).unwrap()),
            vec![ReferenceLineElement::line(length(length_m)).unwrap()],
        ).unwrap();
        let sample = evaluate_reference_line(&curve, length(length_m * fraction), context()).unwrap();
        prop_assert!(sample.position().x_m().get().is_finite());
        prop_assert!(sample.position().y_m().get().is_finite());
        let travelled = libm::hypot(
            sample.position().x_m().get() - 5.0,
            sample.position().y_m().get() + 3.0,
        );
        prop_assert!((travelled - length_m * fraction).abs() <= 1.0e-8);
    }

    #[test]
    fn crossing_intersection_is_order_independent(
        x in -1_000.0_f64..1_000.0,
        y in -1_000.0_f64..1_000.0,
        half_extent in 0.1_f64..100.0,
    ) {
        let horizontal = Segment2::new(
            point(x - half_extent, y),
            point(x + half_extent, y),
        );
        let vertical = Segment2::new(
            point(x, y - half_extent),
            point(x, y + half_extent),
        );
        let first = segment_intersection(horizontal, vertical, context()).unwrap();
        let second = segment_intersection(vertical, horizontal, context()).unwrap();
        prop_assert_eq!(first, second);
    }
}
