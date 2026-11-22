use t2m_core::Knowledge;
use t2m_core::groove::{Band, DrumPattern, Fill, FillSize, Groove, Hit, Lane, SwingPoint};

fn lane(piece: &str, hits: &str) -> Lane {
    Lane {
        piece: piece.into(),
        hits: hits.into(),
        late: 0,
    }
}

fn groove() -> Groove {
    Groove {
        id: "groove.test".into(),
        name: "Test".into(),
        aliases: vec![],
        cycles: vec![],
        meter: (4, 4),
        steps: 8,
        patterns: vec![DrumPattern {
            energy: Band::Low,
            lanes: vec![lane("kick", "X...x...")],
        }],
        fills: vec![Fill {
            size: FillSize::Light,
            lanes: vec![lane("snare", "......xx")],
        }],
        swing: vec![],
        swing_steps: 2,
        timing: vec![],
        accent: vec![],
        human_ticks: 8,
        source: "test".into(),
        verified: false,
    }
}

fn swung(points: &[(u16, u16)]) -> Groove {
    let mut g = groove();
    g.swing = points
        .iter()
        .map(|&(bpm, ratio)| SwingPoint { bpm, ratio })
        .collect();
    g
}

#[test]
fn t11_13_bands_split_energy_into_thirds() {
    assert_eq!(Band::of(0.0), Band::Low);
    assert_eq!(Band::of(0.333), Band::Low);
    assert_eq!(Band::of(1.0 / 3.0), Band::Mid);
    assert_eq!(Band::of(0.5), Band::Mid);
    assert_eq!(Band::of(0.66), Band::Mid);
    assert_eq!(Band::of(2.0 / 3.0), Band::High);
    assert_eq!(Band::of(1.0), Band::High);
    assert!(Band::Low < Band::Mid && Band::Mid < Band::High);
}

#[test]
fn t11_14_lanes_read_their_symbols_and_fills_know_where_they_start() {
    let steps: Vec<_> = lane("kick", "Xxg.?").steps().collect();
    assert_eq!(
        steps,
        [
            Some(Hit::Accent),
            Some(Hit::Normal),
            Some(Hit::Ghost),
            None,
            None
        ]
    );
    assert_eq!(groove().fills[0].first_step(), Some(6));
    let mut two = groove().fills[0].clone();
    two.lanes.push(lane("tom", "..x....."));
    assert_eq!(two.first_step(), Some(2));
    let mut silent = two.clone();
    silent.lanes = vec![lane("tom", "........")];
    assert_eq!(silent.first_step(), None);
}

#[test]
fn t11_15_swing_follows_tempo_between_points_and_holds_beyond_them() {
    assert_eq!(groove().swing_ratio(120), 1000);
    let g = swung(&[(100, 2000), (200, 1000)]);
    assert_eq!(g.swing_ratio(50), 2000);
    assert_eq!(g.swing_ratio(100), 2000);
    assert_eq!(g.swing_ratio(150), 1500);
    assert_eq!(g.swing_ratio(125), 1750);
    assert_eq!(g.swing_ratio(200), 1000);
    assert_eq!(g.swing_ratio(300), 1000);
    let three = swung(&[(60, 2000), (120, 2000), (240, 1000)]);
    assert_eq!(three.swing_ratio(90), 2000);
    assert_eq!(three.swing_ratio(180), 1500);
    assert_eq!(swung(&[(120, 1500)]).swing_ratio(30), 1500);
    assert_eq!(swung(&[(120, 1500)]).swing_ratio(300), 1500);
}

#[test]
fn t11_16_a_groove_that_contradicts_itself_is_refused() {
    assert_eq!(groove().check(), Ok(()));
    let bad = |edit: fn(&mut Groove)| {
        let mut g = groove();
        edit(&mut g);
        g.check().unwrap_err()
    };
    assert!(bad(|g| g.steps = 0).contains("at least one step"));
    assert!(bad(|g| g.patterns[0].lanes[0].hits = "X..".into()).contains("lane kick has 3 steps"));
    assert!(bad(|g| g.fills[0].lanes[0].hits = "......x".into()).contains("lane snare"));
    assert!(bad(|g| g.patterns[0].lanes[0].hits = "X...q...".into()).contains("unknown symbol"));
    assert!(bad(|g| g.fills[0].lanes[0].hits = "........".into()).contains("no hits"));
    assert!(bad(|g| g.timing = vec![0; 3]).contains("timing has 3"));
    assert!(bad(|g| g.accent = vec![0; 9]).contains("accent has 9"));
    let mut ok = groove();
    ok.timing = vec![0; 8];
    ok.accent = vec![0; 8];
    assert_eq!(ok.check(), Ok(()));
    let reason = "swing points must rise";
    assert!(
        swung(&[(120, 1500), (100, 1500)])
            .check()
            .unwrap_err()
            .contains(reason)
    );
    assert!(
        swung(&[(100, 1500), (100, 1500)])
            .check()
            .unwrap_err()
            .contains(reason)
    );
    assert!(swung(&[(100, 999)]).check().unwrap_err().contains(reason));
    assert_eq!(swung(&[(100, 1000), (200, 2000)]).check(), Ok(()));
    assert!(bad(|g| g.swing_steps = 0).contains("swing_steps"));
    assert!(bad(|g| g.swing_steps = 3).contains("swing_steps"));
    let mut four = groove();
    four.swing_steps = 4;
    assert_eq!(four.check(), Ok(()));
}

#[test]
fn t11_17_the_twelve_western_grooves_are_complete() {
    let kb = Knowledge::load().unwrap();
    let ids = [
        "rock_8",
        "pop_four",
        "house",
        "techno",
        "trap",
        "boom_bap",
        "lofi_swing",
        "funk_16",
        "jazz_ride",
        "one_drop",
        "bossa",
        "afrobeats",
    ];
    for id in ids {
        let g = kb.groove(&format!("groove.{id}")).unwrap();
        assert_eq!(g.check(), Ok(()), "{id}");
        for band in [Band::Low, Band::Mid, Band::High] {
            let n = g.patterns.iter().filter(|p| p.energy == band).count();
            assert!(n >= 3, "{id} has {n} {band:?} patterns");
        }
        assert!(g.fills.iter().any(|f| f.size == FillSize::Light), "{id}");
        assert!(g.fills.iter().any(|f| f.size == FillSize::Heavy), "{id}");
        assert!(!g.source.is_empty());
    }
    // Swing feels get a tempo-dependent ratio; straight ones stay straight.
    assert!(kb.groove("groove.jazz_ride").unwrap().swing_ratio(120) > 1500);
    assert_eq!(kb.groove("groove.rock_8").unwrap().swing_ratio(120), 1000);
}

#[test]
fn t11_18_grooves_round_trip_and_default_missing_fields() {
    let g = swung(&[(100, 1500)]);
    let json = serde_json::to_string(&g).unwrap();
    assert_eq!(serde_json::from_str::<Groove>(&json).unwrap(), g);
    let bare: Groove =
        serde_json::from_str(r#"{"id":"g","name":"G","source":"s","verified":false}"#).unwrap();
    assert_eq!(
        (bare.meter, bare.steps, bare.swing_steps, bare.human_ticks),
        ((4, 4), 16, 2, 8)
    );
    assert!(bare.patterns.is_empty() && bare.timing.is_empty());
}
