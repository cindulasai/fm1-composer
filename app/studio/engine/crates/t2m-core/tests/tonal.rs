use proptest::prelude::*;
use t2m_core::Pitch;
use t2m_core::tonal::{MoveGraph, TonalError, TonalFamily, TonalSystem, Tuning};

fn tuning(id: &str, cents: &[i32], period: i32) -> Tuning {
    Tuning {
        id: id.into(),
        cents: cents.to_vec(),
        period,
        source: "test fixture".into(),
        verified: false,
    }
}

fn twelve_tet() -> Tuning {
    let cents: Vec<i32> = (0..12).map(|i| i * 100).collect();
    tuning("12tet", &cents, 1200)
}

fn system(id: &str, steps: &[u8]) -> TonalSystem {
    TonalSystem {
        id: id.into(),
        family: TonalFamily::Mode,
        steps: steps.to_vec(),
        default_tuning: "12tet".into(),
        tunings: vec![],
        aliases: vec![],
        moves: MoveGraph::default(),
        emphasis: vec![],
        rest_degrees: vec![],
        phrases: vec![],
        ornaments: vec![],
        chordable: true,
        source: "test fixture".into(),
        verified: false,
    }
}

fn major() -> TonalSystem {
    system("mode.major", &[0, 2, 4, 5, 7, 9, 11])
}

/// Seven degrees on a table with a neutral third (350) and neutral seventh (1050).
fn neutral() -> (TonalSystem, Tuning) {
    (
        system("test.neutral", &[0, 1, 2, 3, 4, 5, 6]),
        tuning("test.neutral7", &[0, 200, 350, 500, 700, 900, 1050], 1200),
    )
}

#[test]
fn t3_1_degrees_map_to_pitches_in_every_octave() {
    let (major, tet) = (major(), twelve_tet());
    assert_eq!(major.degrees(), 7);
    assert_eq!(major.pitch(&tet, 0), Pitch { cents: 0 });
    assert_eq!(major.pitch(&tet, 2), Pitch { cents: 400 });
    assert_eq!(major.pitch(&tet, 6), Pitch { cents: 1100 });
    assert_eq!(major.pitch(&tet, 7), Pitch { cents: 1200 });
    assert_eq!(major.pitch(&tet, -1), Pitch { cents: -100 });
    assert_eq!(major.pitch(&tet, 9), Pitch { cents: 1600 });
    assert_eq!(major.pitch(&tet, -7), Pitch { cents: -1200 });
    assert_eq!(major.pitch(&tet, -8), Pitch { cents: -1300 });
    // A non-octave period repeats at that period.
    let stretched = tuning("stretched", &[0, 400, 800], 1210);
    let three = system("test.three", &[0, 1, 2]);
    assert_eq!(three.pitch(&stretched, 3), Pitch { cents: 1210 });
    assert_eq!(three.pitch(&stretched, 4), Pitch { cents: 1610 });
    assert_eq!(three.pitch(&stretched, -1), Pitch { cents: -410 });
}

#[test]
fn t3_2_moves_follow_the_ascending_and_descending_forms() {
    let mut raga = major();
    raga.family = TonalFamily::Raga;
    raga.moves = MoveGraph {
        ascending: vec![0, 1, 2, 4, 5, 6],
        descending: vec![0, 1, 2, 3, 4, 5, 6],
    };
    assert!(!raga.legal_move(2, 3));
    assert!(raga.legal_move(4, 3));
    assert!(raga.legal_move(2, 4));
    assert!(raga.legal_move(3, 3), "staying put is always allowed");
    // The rule holds in other octaves.
    assert!(!raga.legal_move(9, 10));
    assert!(!raga.legal_move(-5, -4));
    assert!(raga.legal_move(11, 10));
    assert!(raga.legal_move(-3, -4));
    // Only the descending form restricts downward motion.
    raga.moves.descending = vec![0, 2, 4];
    assert!(!raga.legal_move(4, 3));
    assert!(raga.legal_move(4, 2));
    assert!(raga.legal_move(2, 4));
    // No restriction listed: everything is allowed.
    let free = major();
    assert!(free.legal_move(2, 3) && free.legal_move(3, 2) && free.legal_move(0, 40));
    // A system with no degrees allows anything rather than failing.
    assert!(system("empty", &[]).legal_move(1, 2));
}

#[test]
fn t3_3_nearest_degree_and_ties() {
    let (neutral, table) = neutral();
    assert_eq!(neutral.nearest_degree(&table, Pitch { cents: 340 }), 2);
    assert_eq!(neutral.nearest_degree(&table, Pitch { cents: 350 }), 2);
    assert_eq!(neutral.nearest_degree(&table, Pitch { cents: 1540 }), 9);
    assert_eq!(neutral.nearest_degree(&table, Pitch { cents: -860 }), -5);

    let (major, tet) = (major(), twelve_tet());
    // Halfway between two degrees goes to the lower one.
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 100 }), 0);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 101 }), 1);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 99 }), 0);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 1150 }), 6);
    // Close to the next tonic, and just below this one.
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 1190 }), 7);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 1151 }), 7);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 2390 }), 14);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 20 }), 0);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: -40 }), 0);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: -60 }), -1);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: -1240 }), -7);
    assert_eq!(major.nearest_degree(&tet, Pitch { cents: 1100 }), 6);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn t3_4_nearest_degree_inverts_pitch(degree in -21i32..=21, which in 0usize..3) {
        let (neutral, neutral_table) = neutral();
        let pentatonic = system("mode.major_pentatonic", &[0, 2, 4, 7, 9]);
        let fixtures = [(major(), twelve_tet()), (neutral, neutral_table), (pentatonic, twelve_tet())];
        let (system, tuning) = &fixtures[which];
        let pitch = system.pitch(tuning, degree);
        prop_assert_eq!(system.nearest_degree(tuning, pitch), degree);
    }

    #[test]
    fn t3_4_nearest_degree_is_never_further_than_a_neighbour(cents in -4000i32..=4000) {
        let (major, tet) = (major(), twelve_tet());
        let found = major.nearest_degree(&tet, Pitch { cents });
        let distance = |d: i32| major.pitch(&tet, d).cents.abs_diff(cents);
        prop_assert!(distance(found) <= distance(found - 1));
        prop_assert!(distance(found) <= distance(found + 1));
        prop_assert!(distance(found) <= 100);
    }
}

#[test]
fn t3_5_bad_tunings_and_systems_are_errors() {
    let id = || "t".to_string();
    assert_eq!(
        tuning("t", &[], 1200).check(),
        Err(TonalError::TuningEmpty { id: id() })
    );
    assert_eq!(
        tuning("t", &[10, 200], 1200).check(),
        Err(TonalError::TuningFirst { id: id() })
    );
    assert_eq!(
        tuning("t", &[0, 200, 200], 1200).check(),
        Err(TonalError::TuningOrder { id: id() })
    );
    assert_eq!(
        tuning("t", &[0, 300, 200], 1200).check(),
        Err(TonalError::TuningOrder { id: id() })
    );
    assert_eq!(
        tuning("t", &[0, 700, 1200], 1200).check(),
        Err(TonalError::TuningPeriod { id: id() })
    );
    assert_eq!(
        tuning("t", &[0], 0).check(),
        Err(TonalError::TuningPeriod { id: id() })
    );
    assert_eq!(
        tuning("t", &[0], -1200).check(),
        Err(TonalError::TuningPeriod { id: id() })
    );
    assert_eq!(tuning("t", &[0], 1).check(), Ok(()));
    assert_eq!(twelve_tet().check(), Ok(()));
    assert_eq!(
        tuning("t", &[], 1200).check().unwrap_err().to_string(),
        "tuning t has no pitches"
    );

    let tet = twelve_tet();
    assert_eq!(major().check(&tet), Ok(()));
    let steps_error = |steps: &[u8]| system("s", steps).check(&tet);
    let bad_steps = Err(TonalError::SystemSteps { system: "s".into() });
    assert_eq!(steps_error(&[]), bad_steps);
    assert_eq!(steps_error(&[0, 2, 2]), bad_steps);
    assert_eq!(steps_error(&[0, 4, 2]), bad_steps);
    assert_eq!(steps_error(&[0, 11]), Ok(()));
    assert_eq!(
        steps_error(&[0, 12]),
        Err(TonalError::DegreeMismatch {
            system: "s".into(),
            tuning: "12tet".into()
        })
    );

    type Edit = fn(&mut TonalSystem, Vec<u8>);
    let fields: [(&str, Edit); 4] = [
        ("moves.ascending", |s, v| s.moves.ascending = v),
        ("moves.descending", |s, v| s.moves.descending = v),
        ("emphasis", |s, v| s.emphasis = v),
        ("rest_degrees", |s, v| s.rest_degrees = v),
    ];
    for (field, edit) in fields {
        let mut inside = major();
        edit(&mut inside, vec![0, 6]);
        assert_eq!(inside.check(&tet), Ok(()), "{field}");
        let mut outside = major();
        edit(&mut outside, vec![0, 7]);
        assert_eq!(
            outside.check(&tet),
            Err(TonalError::DegreeOutside {
                system: "mode.major".into(),
                field,
                degree: 7
            })
        );
    }
}

#[test]
fn t3_6_mismatched_or_extreme_input_never_panics() {
    let major = major();
    // A table too short for the system: missing pitches read as 0 cents.
    let short = tuning("short", &[0, 100, 200], 1200);
    assert_eq!(major.pitch(&short, 1), Pitch { cents: 200 });
    assert_eq!(major.pitch(&short, 2), Pitch { cents: 0 });
    assert_eq!(major.pitch(&short, 9), Pitch { cents: 1200 });
    let _ = major.nearest_degree(&short, Pitch { cents: 5000 });

    let tet = twelve_tet();
    assert_eq!(major.pitch(&tet, i32::MAX), Pitch { cents: i32::MAX });
    assert_eq!(major.pitch(&tet, i32::MIN), Pitch { cents: i32::MIN });
    for cents in [i32::MIN, i32::MAX, -1, 0] {
        let _ = major.nearest_degree(&tet, Pitch { cents });
    }
    assert!(major.legal_move(i32::MIN, i32::MAX));

    // No degrees at all.
    let empty = system("empty", &[]);
    assert_eq!(empty.degrees(), 0);
    assert_eq!(empty.pitch(&tet, 5), Pitch { cents: 0 });
    assert_eq!(empty.nearest_degree(&tet, Pitch { cents: 700 }), 0);

    // A period of zero or less is read as an octave.
    for period in [0, -5] {
        let broken = tuning(
            "broken",
            &[0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100],
            period,
        );
        assert_eq!(major.pitch(&broken, 7), Pitch { cents: 1200 });
        assert_eq!(major.nearest_degree(&broken, Pitch { cents: 1210 }), 7);
    }
    // More steps than a u8 can count.
    let huge = TonalSystem {
        steps: vec![0; 300],
        ..system("huge", &[])
    };
    assert_eq!(huge.degrees(), 255);
    let _ = huge.pitch(&tet, 299);
}

#[test]
fn tonal_types_deserialize_with_defaults() {
    let bare = r#"{"id":"mode.major","family":"Mode","steps":[0,2,4,5,7,9,11],
        "default_tuning":"12tet","chordable":true,"source":"s","verified":false}"#;
    let parsed: TonalSystem = serde_json::from_str(bare).unwrap();
    assert_eq!(parsed, major_with_source("s"));
}

fn major_with_source(source: &str) -> TonalSystem {
    TonalSystem {
        source: source.into(),
        ..major()
    }
}
