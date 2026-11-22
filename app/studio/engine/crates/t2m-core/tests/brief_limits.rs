use t2m_core::brief::{Brief, Length, MeterSpec};
use t2m_core::brief_ops::{
    MAX_BAR_QUARTERS, MAX_BARS, MAX_PIECE_BEATS, MAX_PIECE_QUARTERS, repair, validate,
};
use t2m_core::{Knowledge, Meter, TICKS_PER_QUARTER};

fn kb() -> Knowledge {
    Knowledge::load().unwrap()
}

fn base() -> Brief {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../suite/briefs/pop-generic-1.json"
    ))
    .unwrap();
    Brief::from_json(&text).unwrap()
}

/// The piece's length in ticks after repair, as the form says it.
fn ticks_of(brief: &Brief) -> i64 {
    let MeterSpec::Meter {
        numerator,
        denominator,
        ..
    } = &brief.time.meter
    else {
        unreachable!("test briefs use plain meters");
    };
    let bar = Meter::new(*numerator, *denominator, None)
        .unwrap()
        .bar_ticks();
    brief.form.iter().map(|s| i64::from(s.bars)).sum::<i64>() * bar
}

#[test]
fn t40_1_t43_1_no_meter_or_length_can_make_a_piece_bigger_than_the_limit() {
    let kb = kb();
    for (numerator, denominator) in [
        (255, 1),
        (255, 4),
        (255, 32),
        (32, 4),
        (64, 2),
        (16, 16),
        (7, 8),
        (4, 4),
        (1, 32),
    ] {
        for length in [
            Length::Bars(512),
            Length::Bars(65_535),
            Length::Seconds(65_535),
            Length::Cycles(512),
        ] {
            let mut brief = base();
            brief.time.meter = MeterSpec::Meter {
                numerator,
                denominator,
                grouping: None,
            };
            brief.time.length = length;
            let original = brief.form.len();
            for section in &mut brief.form {
                section.bars = u16::MAX;
            }
            while brief.form.len() < 24 {
                brief.form.push(brief.form[original - 1].clone());
                let n = brief.form.len();
                brief.form[n - 1].id = format!("extra{n}");
            }
            let (fixed, _) = repair(brief, &kb);
            assert!(
                validate(&fixed, &kb).is_ok(),
                "{numerator}/{denominator} {length:?}"
            );
            let MeterSpec::Meter {
                numerator: n,
                denominator: d,
                ..
            } = &fixed.time.meter
            else {
                panic!("meter changed kind");
            };
            let bar = Meter::new(*n, *d, None).unwrap().bar_ticks();
            assert!(
                bar <= MAX_BAR_QUARTERS * TICKS_PER_QUARTER,
                "{n}/{d} is a {bar}-tick bar"
            );
            assert!(
                ticks_of(&fixed) <= MAX_PIECE_QUARTERS * TICKS_PER_QUARTER,
                "{numerator}/{denominator} {length:?} -> {}",
                ticks_of(&fixed)
            );
            let total_bars: i64 = fixed.form.iter().map(|s| i64::from(s.bars)).sum();
            assert!(
                total_bars * i64::from(*n) <= MAX_PIECE_BEATS,
                "{n}/{d}: {total_bars} bars is too many beats"
            );
            assert!(fixed.form.iter().all(|s| s.bars >= 1 && s.bars <= MAX_BARS));
            assert!(!fixed.form.is_empty());
            if let Length::Bars(count) | Length::Cycles(count) = fixed.time.length {
                assert!(i64::from(count) * bar <= MAX_PIECE_QUARTERS * TICKS_PER_QUARTER);
            }
        }
    }
}

#[test]
fn t40_2_ordinary_pieces_are_left_alone_and_the_biggest_ordinary_piece_still_fits() {
    let kb = kb();
    let brief = base();
    let (same, issues) = repair(brief.clone(), &kb);
    assert_eq!(same, brief);
    assert!(issues.is_empty(), "{issues:?}");
    // 512 bars of 4/4 is exactly the limit and is kept whole.
    let mut big = base();
    big.time.length = Length::Bars(512);
    let n = big.form.len();
    big.form[n - 1].bars = 512 - big.form[..n - 1].iter().map(|s| s.bars).sum::<u16>();
    let (kept, _) = repair(big, &kb);
    assert_eq!(
        kept.form.iter().map(|s| u32::from(s.bars)).sum::<u32>(),
        512
    );
}

#[test]
fn t40_3_a_huge_title_or_summary_is_cut_and_reported() {
    let kb = kb();
    let mut brief = base();
    brief.identity.title = "t".repeat(100_000);
    brief.identity.summary = "s".repeat(100_000);
    let (fixed, issues) = repair(brief, &kb);
    assert_eq!(fixed.identity.title.chars().count(), 200);
    assert_eq!(fixed.identity.summary.chars().count(), 1_000);
    assert!(issues.iter().any(|i| i.path == "/identity/title"));
    assert!(issues.iter().any(|i| i.path == "/identity/summary"));
    // Exactly at the limit is untouched; multi-byte text is counted in characters.
    let mut brief = base();
    brief.identity.title = "é".repeat(200);
    let (same, issues) = repair(brief, &kb);
    assert_eq!(same.identity.title.chars().count(), 200);
    assert!(issues.is_empty(), "{issues:?}");
}
