use proptest::prelude::*;
use t2m_core::{Pitch, PitchClass, TICKS_PER_QUARTER, Tick, Tonic, parse_pitch_class};

#[test]
fn t1_1_tick_resolution_is_960_and_tick_is_i64() {
    assert_eq!(TICKS_PER_QUARTER, 960);
    let widest: Tick = i64::MAX;
    assert_eq!(widest, i64::MAX);
}

#[test]
fn t1_2_note_names_parse_to_pitch_classes() {
    assert_eq!(parse_pitch_class("Db"), parse_pitch_class("C#"));
    assert_eq!(parse_pitch_class("C#"), Some(PitchClass::Cs));
    assert_eq!(parse_pitch_class("E♭"), Some(PitchClass::Ds));
    assert_eq!(parse_pitch_class("f#"), Some(PitchClass::Fs));
    assert_eq!(parse_pitch_class("F♯"), Some(PitchClass::Fs));
    assert_eq!(parse_pitch_class("Cb"), Some(PitchClass::B));
    assert_eq!(parse_pitch_class("B#"), Some(PitchClass::C));
    assert_eq!(parse_pitch_class("bb"), Some(PitchClass::As));
    assert_eq!(parse_pitch_class("Bbb"), Some(PitchClass::A));
    assert_eq!(parse_pitch_class("C##"), Some(PitchClass::D));
    assert_eq!(parse_pitch_class("  G "), Some(PitchClass::G));
    let all = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    for (i, name) in all.iter().enumerate() {
        let class = parse_pitch_class(name).unwrap();
        assert_eq!(usize::from(class.semitone()), i);
        assert_eq!(class.name(), *name);
        assert_eq!(PitchClass::from_semitone(i as i64), class);
    }
    assert_eq!(PitchClass::from_semitone(-1), PitchClass::B);
    assert_eq!(PitchClass::from_semitone(25), PitchClass::Cs);
}

#[test]
fn t1_3_bad_note_names_are_none() {
    let long = "C".repeat(10_000);
    let cases = [
        "H",
        "",
        " ",
        "C##x",
        "C###",
        "C#b",
        "Cb#",
        "Do",
        "1",
        "#",
        "♭",
        "🎹",
        "C\0",
        "\0",
        "C #",
        long.as_str(),
    ];
    for case in cases {
        assert_eq!(parse_pitch_class(case), None, "input {case:?}");
    }
}

#[test]
fn t1_4_pitch_to_midi_rounds_quarter_tones_up() {
    assert_eq!(Pitch { cents: 350 }.to_midi(60), (64, -50.0));
    assert_eq!(Pitch { cents: 1200 }.to_midi(60), (72, 0.0));
    assert_eq!(Pitch { cents: -50 }.to_midi(60), (60, -50.0));
    assert_eq!(Pitch { cents: 0 }.to_midi(60), (60, 0.0));
    assert_eq!(Pitch { cents: 49 }.to_midi(60), (60, 49.0));
    assert_eq!(Pitch { cents: -51 }.to_midi(60), (59, 49.0));
    assert_eq!(Pitch { cents: 1 }.to_midi(0), (0, 1.0));
}

#[test]
fn t1_5_pitch_to_midi_clamps_note_and_offset() {
    for cents in [1_000_000, i32::MAX, 6_800, 6_751] {
        assert_eq!(Pitch { cents }.to_midi(60), (127, 50.0), "cents {cents}");
    }
    for cents in [-1_000_000, i32::MIN, -6_051] {
        assert_eq!(Pitch { cents }.to_midi(60), (0, -50.0), "cents {cents}");
    }
    assert_eq!(Pitch { cents: 6_700 }.to_midi(60), (127, 0.0));
    assert_eq!(Pitch { cents: 6_749 }.to_midi(60), (127, 49.0));
    assert_eq!(Pitch { cents: 6_750 }.to_midi(60), (127, 50.0));
    assert_eq!(Pitch { cents: -6_000 }.to_midi(60), (0, 0.0));
    assert_eq!(Pitch { cents: -6_050 }.to_midi(60), (0, -50.0));
    assert_eq!(Pitch { cents: i32::MAX }.to_midi(255), (127, 50.0));
}

#[test]
fn t1_6_tonic_resolves_to_midi_note_and_offset() {
    assert_eq!(Tonic::Hz(440.0).midi_and_offset(4), (69, 0.0));
    assert_eq!(Tonic::Hz(220.0).midi_and_offset(4), (69, 0.0));
    assert_eq!(Tonic::Class(PitchClass::C).midi_and_offset(4), (60, 0.0));
    assert_eq!(Tonic::Class(PitchClass::A).midi_and_offset(4), (69, 0.0));
    assert_eq!(Tonic::Class(PitchClass::C).midi_and_offset(-1), (0, 0.0));
    assert_eq!(Tonic::Class(PitchClass::C).midi_and_offset(-5), (0, 0.0));
    assert_eq!(Tonic::Class(PitchClass::G).midi_and_offset(9), (127, 0.0));
    assert_eq!(Tonic::Class(PitchClass::Gs).midi_and_offset(9), (116, 0.0));
    assert_eq!(Tonic::Class(PitchClass::C).midi_and_offset(99), (120, 0.0));

    // 20 cents sharp of A stays on A.
    let (note, offset) = Tonic::Hz(445.112).midi_and_offset(4);
    assert_eq!(note, 69);
    assert!((offset - 20.0).abs() < 0.05, "offset {offset}");

    // Just past the quarter-tone above A belongs to A#.
    let (note, offset) = Tonic::Hz(452.9).midi_and_offset(4);
    assert_eq!(note, 70);
    assert!((-50.0..-49.0).contains(&offset), "offset {offset}");

    // Just below it still belongs to A.
    let (note, offset) = Tonic::Hz(452.8).midi_and_offset(4);
    assert_eq!(note, 69);
    assert!(offset > 49.0, "offset {offset}");
    assert!(offset <= 50.0, "offset {offset}");
}

#[test]
fn t1_7_unusable_frequencies_fall_back_to_c() {
    for hz in [0.0, -440.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Tonic::Hz(hz).midi_and_offset(4), (60, 0.0), "hz {hz}");
    }
    // Extreme but valid frequencies still give a note in range.
    for hz in [f64::MIN_POSITIVE, 1e-300, 1e300, f64::MAX] {
        let (note, offset) = Tonic::Hz(hz).midi_and_offset(4);
        assert!((60..72).contains(&note), "hz {hz} note {note}");
        assert!((-50.0..=50.0).contains(&offset), "hz {hz} offset {offset}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn t1_8_note_plus_offset_equals_tonic_plus_cents(cents in -6000i32..=6000, tonic in 48u8..=72) {
        let (note, offset) = Pitch { cents }.to_midi(tonic);
        let expected = i64::from(tonic) * 100 + i64::from(cents);
        let clamped = !(-50..=12_750).contains(&expected);
        prop_assert!((-50.0..=50.0).contains(&offset));
        if !clamped {
            let got = f64::from(note) * 100.0 + f64::from(offset);
            prop_assert!((got - expected as f64).abs() < 0.01, "got {} expected {}", got, expected);
        }
    }
}

#[test]
fn t1_9_octave_up_adds_1200_cents_and_saturates() {
    assert_eq!(Pitch { cents: 100 }.octave_up(1), Pitch { cents: 1300 });
    assert_eq!(Pitch { cents: 100 }.octave_up(-2), Pitch { cents: -2300 });
    assert_eq!(Pitch { cents: 100 }.octave_up(0), Pitch { cents: 100 });
    assert_eq!(
        Pitch { cents: 5 }.octave_up(i32::MAX),
        Pitch { cents: i32::MAX }
    );
    assert_eq!(
        Pitch { cents: -5 }.octave_up(i32::MIN),
        Pitch { cents: i32::MIN }
    );
    assert_eq!(
        Pitch { cents: i32::MAX }.octave_up(1),
        Pitch { cents: i32::MAX }
    );
}

#[test]
fn pitch_and_tonic_serialize() {
    let pitch = Pitch { cents: 350 };
    let json = serde_json::to_string(&pitch).unwrap();
    assert_eq!(serde_json::from_str::<Pitch>(&json).unwrap(), pitch);
    let tonic = Tonic::Hz(146.83);
    let json = serde_json::to_string(&tonic).unwrap();
    assert_eq!(serde_json::from_str::<Tonic>(&json).unwrap(), tonic);
}
