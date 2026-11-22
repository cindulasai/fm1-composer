use t2m_core::time::{Accent, Cycle, Meter, Stroke, TimeError, TimeFrame};

fn meter(n: u8, d: u8) -> Meter {
    Meter::new(n, d, None).unwrap()
}

fn cycle(beats: u8, beat_unit: u8, grouping: &[u8]) -> Cycle {
    Cycle {
        id: "cycle.test".into(),
        beats,
        beat_unit,
        grouping: grouping.to_vec(),
        accents: vec![],
        strokes: vec![],
        source: "test fixture".into(),
        verified: false,
    }
}

#[test]
fn t2_1_bar_lengths_and_default_groupings() {
    assert_eq!(meter(4, 4).bar_ticks(), 3840);
    assert_eq!(meter(4, 4).grouping, [1, 1, 1, 1]);
    assert_eq!(meter(3, 4).bar_ticks(), 2880);
    assert_eq!(meter(6, 8).bar_ticks(), 2880);
    assert_eq!(meter(6, 8).grouping, [3, 3]);
    assert_eq!(meter(12, 8).grouping, [3, 3, 3, 3]);
    assert_eq!(meter(9, 16).grouping, [3, 3, 3]);
    assert_eq!(meter(5, 4).grouping, [1, 1, 1, 1, 1]);
    assert_eq!(meter(6, 4).grouping, [1, 1, 1, 1, 1, 1]);
    assert_eq!(meter(3, 8).grouping, [1, 1, 1]);
    assert_eq!(meter(2, 8).grouping, [1, 1]);
    assert_eq!(meter(4, 8).grouping, [1, 1, 1, 1]);
    assert_eq!(meter(5, 8).grouping, [2, 3]);
    assert_eq!(meter(7, 8).grouping, [2, 2, 3]);
    assert_eq!(meter(11, 8).grouping, [2, 2, 2, 2, 3]);
    assert_eq!(meter(7, 4).grouping, [1; 7]);
    assert_eq!(meter(1, 1).bar_ticks(), 3840);
    assert_eq!(meter(2, 2).bar_ticks(), 3840);
    assert_eq!(meter(1, 32).bar_ticks(), 120);
    assert_eq!(meter(255, 1).bar_ticks(), 255 * 3840);
    assert_eq!(Meter::default_grouping(7, 8), [2, 2, 3]);
    assert_eq!(meter(4, 4).accents(), [0, 960, 1920, 2880]);
}

#[test]
fn t2_2_additive_meter_seven_eight() {
    let seven = Meter::new(7, 8, Some(vec![2, 2, 3])).unwrap();
    assert_eq!(seven.bar_ticks(), 3360);
    assert_eq!(seven.accents(), [0, 960, 1920]);
    let other = Meter::new(7, 8, Some(vec![3, 2, 2])).unwrap();
    assert_eq!(other.accents(), [0, 1440, 2400]);
    assert_eq!(TimeFrame::Meter(other).strong_positions(), [0, 1440, 2400]);
}

#[test]
fn t2_3_bad_meters_are_errors() {
    assert_eq!(
        Meter::new(7, 8, Some(vec![2, 2, 2])),
        Err(TimeError::GroupingSum {
            expected: 7,
            got: 6
        })
    );
    assert_eq!(
        Meter::new(4, 4, Some(vec![255, 255])),
        Err(TimeError::GroupingSum {
            expected: 4,
            got: 510
        })
    );
    for denominator in [0, 3, 5, 6, 12, 64, 255] {
        assert_eq!(
            Meter::new(4, denominator, None),
            Err(TimeError::Denominator(denominator)),
            "denominator {denominator}"
        );
    }
    assert_eq!(Meter::new(0, 4, None), Err(TimeError::Numerator));
    assert_eq!(
        Meter::new(4, 4, Some(vec![2, 0, 2])),
        Err(TimeError::GroupingEmpty)
    );
    assert_eq!(
        Meter::new(4, 4, Some(vec![])),
        Err(TimeError::GroupingEmpty)
    );
    // A meter built by hand with a bad denominator still has a positive bar.
    let by_hand = Meter {
        numerator: 0,
        denominator: 0,
        grouping: vec![],
    };
    assert_eq!(by_hand.bar_ticks(), 960);
    assert_eq!(TimeFrame::Meter(by_hand).strong_positions(), [0]);
    let message = TimeError::GroupingSum {
        expected: 7,
        got: 6,
    }
    .to_string();
    assert_eq!(message, "the grouping adds up to 6, not 7");
}

#[test]
fn t2_4_cycle_span_and_strong_positions() {
    let rupak = cycle(7, 4, &[3, 2, 2]);
    assert_eq!(rupak.check(), Ok(()));
    let frame = TimeFrame::Cycle(rupak);
    assert_eq!(frame.span_ticks(), 6720);
    assert_eq!(frame.strong_positions(), [0, 2880, 4800]);
    let eighths = TimeFrame::Cycle(cycle(7, 8, &[3, 2, 2]));
    assert_eq!(eighths.span_ticks(), 3360);
    assert_eq!(eighths.strong_positions(), [0, 1440, 2400]);
}

#[test]
fn t2_5_meter_strong_positions() {
    let strong = |n, d| TimeFrame::Meter(meter(n, d)).strong_positions();
    assert_eq!(strong(4, 4), [0, 1920]);
    assert_eq!(strong(3, 4), [0]);
    assert_eq!(strong(2, 4), [0]);
    assert_eq!(strong(5, 4), [0]);
    assert_eq!(strong(6, 4), [0, 2880]);
    assert_eq!(strong(6, 8), [0, 1440]);
    assert_eq!(strong(7, 8), [0, 960, 1920]);
    assert_eq!(TimeFrame::Meter(meter(4, 4)).span_ticks(), 3840);
}

#[test]
fn t2_6_bad_cycles_are_errors_and_spans_stay_positive() {
    assert_eq!(cycle(0, 4, &[]).check(), Err(TimeError::Beats));
    assert_eq!(
        cycle(7, 4, &[3, 2]).check(),
        Err(TimeError::GroupingSum {
            expected: 7,
            got: 5
        })
    );
    assert_eq!(cycle(7, 4, &[]).check(), Err(TimeError::GroupingEmpty));
    assert_eq!(cycle(7, 3, &[7]).check(), Err(TimeError::Denominator(3)));

    let mut strokes = cycle(3, 4, &[3]);
    strokes.strokes = vec![Stroke { name: "dha".into() }; 2];
    assert_eq!(
        strokes.check(),
        Err(TimeError::StrokeLength {
            expected: 3,
            got: 2
        })
    );
    strokes.strokes.push(Stroke { name: "na".into() });
    assert_eq!(strokes.check(), Ok(()));

    let mut accents = cycle(3, 4, &[3]);
    accents.accents = vec![Accent { beat: 2, weight: 1 }];
    assert_eq!(accents.check(), Ok(()));
    accents.accents.push(Accent { beat: 3, weight: 1 });
    assert_eq!(
        accents.check(),
        Err(TimeError::AccentOutside { beat: 3, beats: 3 })
    );

    // Unchecked cycles never give a zero or negative span.
    for broken in [cycle(0, 4, &[]), cycle(0, 0, &[0]), cycle(5, 7, &[9])] {
        assert!(TimeFrame::Cycle(broken).span_ticks() > 0);
    }
    assert_eq!(TimeFrame::Cycle(cycle(0, 0, &[])).span_ticks(), 960);
    for beats in 1..=255u8 {
        for unit in [1u8, 2, 4, 8, 16, 32] {
            let checked = cycle(beats, unit, &[beats]);
            assert_eq!(checked.check(), Ok(()));
            assert!(TimeFrame::Cycle(checked).span_ticks() > 0);
        }
    }
}

#[test]
fn time_types_serialize() {
    let frame = TimeFrame::Meter(meter(7, 8));
    let json = serde_json::to_string(&frame).unwrap();
    assert_eq!(serde_json::from_str::<TimeFrame>(&json).unwrap(), frame);
    let bare = r#"{"id":"c","beats":3,"beat_unit":4,"grouping":[3],"source":"s","verified":false}"#;
    let parsed: Cycle = serde_json::from_str(bare).unwrap();
    assert!(parsed.accents.is_empty() && parsed.strokes.is_empty());
}
