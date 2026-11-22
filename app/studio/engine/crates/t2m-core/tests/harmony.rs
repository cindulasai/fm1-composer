use t2m_core::Pitch;
use t2m_core::harmony::{Chord, HarmonyError, parse_roman};
use t2m_core::tonal::{MoveGraph, TonalFamily, TonalSystem, Tuning};

fn twelve_tet() -> Tuning {
    Tuning {
        id: "12tet".into(),
        cents: (0..12).map(|i| i * 100).collect(),
        period: 1200,
        source: "test fixture".into(),
        verified: false,
    }
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

fn minor() -> TonalSystem {
    system("mode.minor", &[0, 2, 3, 5, 7, 8, 10])
}

fn chord_in(system: &TonalSystem, symbol: &str) -> Chord {
    parse_roman(symbol, system, &twelve_tet())
        .unwrap_or_else(|error| panic!("{symbol:?} should parse: {error}"))
}

fn tones_in(system: &TonalSystem, symbol: &str) -> Vec<i32> {
    chord_in(system, symbol)
        .tones
        .iter()
        .map(|pitch| pitch.cents)
        .collect()
}

fn tones(symbol: &str) -> Vec<i32> {
    tones_in(&major(), symbol)
}

#[test]
fn t4_1_diatonic_chords_in_c_major() {
    assert_eq!(tones("V7"), [700, 1100, 1400, 1700]);
    assert_eq!(tones("ii"), [200, 500, 900]);
    assert_eq!(tones("IVmaj7"), [500, 900, 1200, 1600]);
    assert_eq!(tones("vii°"), [1100, 1400, 1700]);
    assert_eq!(tones("viidim"), [1100, 1400, 1700]);
    assert_eq!(tones("I"), [0, 400, 700]);
    assert_eq!(tones("vi"), [900, 1200, 1600]);
    assert_eq!(tones("iii"), [400, 700, 1100]);
    assert_eq!(tones("ii7"), [200, 500, 900, 1200]);
    assert_eq!(tones("vii°7"), [1100, 1400, 1700, 2000]);
    assert_eq!(tones("viiø"), [1100, 1400, 1700, 2100]);
    assert_eq!(tones("viiø7"), [1100, 1400, 1700, 2100]);
    assert_eq!(tones("  V7 "), [700, 1100, 1400, 1700]);

    let five = chord_in(&major(), " V7 ");
    assert_eq!(five.root_degree, 4);
    assert_eq!(five.label, "V7");
    assert_eq!(five.bass, Pitch { cents: 700 });
    for (index, numeral) in ["I", "II", "III", "IV", "V", "VI", "VII"]
        .iter()
        .enumerate()
    {
        assert_eq!(chord_in(&major(), numeral).root_degree, index as i32);
    }
}

#[test]
fn t4_2_accidentals_and_case() {
    let flat_seven = chord_in(&major(), "bVII");
    assert_eq!(flat_seven.tones[0], Pitch { cents: 1000 });
    assert_eq!(tones("bVII"), [1000, 1400, 1700]);
    assert_eq!(tones("♭VII"), [1000, 1400, 1700]);
    assert_eq!(tones("#iv"), [600, 900, 1300]);
    assert_eq!(tones("♯iv°"), [600, 900, 1200]);
    assert_eq!(tones("i"), [0, 300, 700]);
    assert_eq!(tones("IVm"), [500, 800, 1200]);
    assert_eq!(tones("ivm"), [500, 800, 1200]);
    assert_eq!(tones("bbVII"), [900, 1300, 1600]);
    assert_eq!(tones("bI"), [1100, 1500, 1800]);
    assert_eq!(tones("#VII"), [0, 400, 700]);
    // Opposite accidentals cancel.
    assert_eq!(tones("b#V"), [700, 1100, 1400]);
}

#[test]
fn t4_3_applied_chords() {
    assert_eq!(tones("V/V"), [200, 600, 900]);
    assert_eq!(tones("V7/ii"), [900, 1300, 1600, 1900]);
    assert_eq!(tones("V7/IV"), [0, 400, 700, 1000]);
    assert_eq!(tones("vii°/V"), [600, 900, 1200]);
    assert_eq!(tones("IV/IV"), [1000, 1400, 1700]);
    assert_eq!(tones("V/bVII"), [500, 900, 1200]);
    assert_eq!(tones("bII/V"), [800, 1200, 1500]);
    assert_eq!(chord_in(&major(), "V/V").root_degree, 1);
    assert_eq!(chord_in(&major(), "V7/ii").root_degree, 5);
    assert_eq!(chord_in(&major(), "V/V").bass, Pitch { cents: 200 });
    assert_eq!(chord_in(&major(), "V/V").label, "V/V");
}

#[test]
fn t4_4_suspensions_and_extensions() {
    assert_eq!(tones("Isus4"), [0, 500, 700]);
    assert_eq!(tones("Isus2"), [0, 200, 700]);
    assert_eq!(tones("I6"), [0, 400, 700, 900]);
    assert_eq!(tones("i6"), [0, 300, 700, 900]);
    assert_eq!(tones("Iadd9"), [0, 400, 700, 1400]);
    assert_eq!(tones("V9"), [700, 1100, 1400, 1700, 2100]);
    assert_eq!(tones("Imaj9"), [0, 400, 700, 1100, 1400]);
    assert_eq!(tones("ii11"), [200, 500, 900, 1200, 1600, 1900]);
    assert_eq!(tones("V13"), [700, 1100, 1400, 1700, 2100, 2800]);
    assert_eq!(tones("IIIaug"), [400, 800, 1200]);
    assert_eq!(tones("III+"), [400, 800, 1200]);
    assert_eq!(tones("III+7"), [400, 800, 1200, 1400]);
    assert_eq!(tones("Iaug6"), [0, 400, 800, 900]);
    assert_eq!(tones("V7sus4"), [700, 1200, 1400, 1700]);
    assert_eq!(tones("i7"), [0, 300, 700, 1000]);
    assert_eq!(tones("imaj7"), [0, 300, 700, 1100]);
}

#[test]
fn t4_5_slash_bass_names_the_lowest_chord_tone() {
    let first_inversion = chord_in(&major(), "V/3");
    assert_eq!(first_inversion.bass, Pitch { cents: 1100 });
    assert_eq!(tones("V/3"), [700, 1100, 1400]);
    assert_eq!(first_inversion.root_degree, 4);
    assert_eq!(chord_in(&major(), "V/5").bass, Pitch { cents: 1400 });
    assert_eq!(chord_in(&major(), "V7/7").bass, Pitch { cents: 1700 });
    assert_eq!(chord_in(&major(), "Imaj7/7").bass, Pitch { cents: 1100 });
    assert_eq!(chord_in(&major(), "ii/3").bass, Pitch { cents: 500 });
    assert_eq!(chord_in(&major(), "vii°/5").bass, Pitch { cents: 1700 });
    assert_eq!(chord_in(&major(), "Isus4/3").bass, Pitch { cents: 500 });
    assert_eq!(chord_in(&major(), "IV").bass, Pitch { cents: 500 });
    let twelve = twelve_tet();
    assert_eq!(
        parse_roman("V/7", &major(), &twelve),
        Err(HarmonyError::Slash("7".into()))
    );
    assert_eq!(
        parse_roman("I6/7", &major(), &twelve),
        Err(HarmonyError::Slash("7".into()))
    );
}

#[test]
fn t4_6_bad_symbols_are_errors() {
    let twelve = twelve_tet();
    let parse = |symbol: &str| parse_roman(symbol, &major(), &twelve);
    assert_eq!(parse(""), Err(HarmonyError::Empty));
    assert_eq!(parse("   "), Err(HarmonyError::Empty));
    assert_eq!(parse("VIII"), Err(HarmonyError::Numeral("VIII".into())));
    assert_eq!(parse("IIII"), Err(HarmonyError::Numeral("IIII".into())));
    assert_eq!(parse("Iv"), Err(HarmonyError::Numeral("Iv".into())));
    assert_eq!(parse("7"), Err(HarmonyError::Numeral("7".into())));
    assert_eq!(parse("bbbV"), Err(HarmonyError::Numeral("bbbV".into())));
    assert_eq!(parse("b"), Err(HarmonyError::Numeral("b".into())));
    assert_eq!(parse("C"), Err(HarmonyError::Numeral("C".into())));
    assert_eq!(parse("V77"), Err(HarmonyError::Suffix("77".into())));
    assert_eq!(parse("Vxyz"), Err(HarmonyError::Suffix("xyz".into())));
    assert_eq!(parse("V7x"), Err(HarmonyError::Suffix("7x".into())));
    assert_eq!(parse("Vsus4x"), Err(HarmonyError::Suffix("sus4x".into())));
    assert_eq!(parse("Vsus"), Err(HarmonyError::Suffix("sus".into())));
    assert_eq!(parse("vii°9"), Err(HarmonyError::Suffix("°9".into())));
    assert_eq!(parse("viiø9"), Err(HarmonyError::Suffix("ø9".into())));
    assert_eq!(parse("vii°sus4"), Err(HarmonyError::Suffix("°sus4".into())));
    assert_eq!(parse("III+sus2"), Err(HarmonyError::Suffix("+sus2".into())));
    assert_eq!(parse("V 7"), Err(HarmonyError::Suffix(" 7".into())));
    assert_eq!(parse("/"), Err(HarmonyError::Numeral("".into())));
    assert_eq!(parse("V/"), Err(HarmonyError::Slash("".into())));
    assert_eq!(parse("V/9"), Err(HarmonyError::Slash("9".into())));
    assert_eq!(parse("V/V7"), Err(HarmonyError::Slash("V7".into())));
    assert_eq!(parse("V/V/3"), Err(HarmonyError::Slash("V/3".into())));
    assert_eq!(parse("V/x"), Err(HarmonyError::Slash("x".into())));
    assert_eq!(parse("🎹"), Err(HarmonyError::Numeral("🎹".into())));
    assert_eq!(parse("V🎹"), Err(HarmonyError::Suffix("🎹".into())));
    assert_eq!(parse("V\0"), Err(HarmonyError::Suffix("\0".into())));
    assert_eq!(parse(&"V".repeat(10_000)), Err(HarmonyError::TooLong));
    assert_eq!(parse(&"V".repeat(25)), Err(HarmonyError::TooLong));
    assert_eq!(parse(&"♭".repeat(25)), Err(HarmonyError::TooLong));
    // Exactly at the limit the symbol is read (and rejected for what it is).
    assert_eq!(
        parse(&"V".repeat(24)),
        Err(HarmonyError::Numeral("V".repeat(24)))
    );
    assert_eq!(
        parse(&"🎹".repeat(24)),
        Err(HarmonyError::Numeral("🎹".repeat(24)))
    );
    assert_eq!(
        HarmonyError::TooLong.to_string(),
        "the chord symbol is longer than 24 characters"
    );
}

#[test]
fn t4_7_systems_without_chords_refuse() {
    let mut raga = major();
    raga.id = "raga.test".into();
    raga.chordable = false;
    let result = parse_roman("I", &raga, &twelve_tet());
    assert_eq!(
        result,
        Err(HarmonyError::NotChordable {
            system: "raga.test".into()
        })
    );
    // The refusal comes before any reading of the symbol.
    assert_eq!(
        parse_roman("", &raga, &twelve_tet()),
        Err(HarmonyError::NotChordable {
            system: "raga.test".into()
        })
    );
}

#[test]
fn t4_8_roots_follow_the_mode_and_case_gives_quality() {
    let minor = minor();
    assert_eq!(tones_in(&minor, "i"), [0, 300, 700]);
    assert_eq!(tones_in(&minor, "III"), [300, 700, 1000]);
    assert_eq!(tones_in(&minor, "V"), [700, 1100, 1400]);
    assert_eq!(tones_in(&minor, "VII"), [1000, 1400, 1700]);
    assert_eq!(tones_in(&minor, "bVII"), [1000, 1400, 1700]);
    assert_eq!(tones_in(&minor, "VI"), [800, 1200, 1500]);
    assert_eq!(tones_in(&minor, "bVI"), [800, 1200, 1500]);
    assert_eq!(tones_in(&minor, "#vi°"), [1000, 1300, 1600]);
    assert_eq!(tones_in(&minor, "V/III"), [1000, 1400, 1700]);
    assert_eq!(chord_in(&minor, "VII").root_degree, 6);
    assert_eq!(chord_in(&minor, "V/III").root_degree, 6);

    let phrygian = system("mode.phrygian", &[0, 1, 3, 5, 7, 8, 10]);
    assert_eq!(tones_in(&phrygian, "II"), [100, 500, 800]);
    assert_eq!(tones_in(&phrygian, "bII"), [100, 500, 800]);

    // Without seven degrees, numerals are read against the major scale.
    let pentatonic = system("mode.major_pentatonic", &[0, 2, 4, 7, 9]);
    assert_eq!(tones_in(&pentatonic, "IV"), [500, 900, 1200]);
    assert_eq!(tones_in(&pentatonic, "vi"), [900, 1200, 1600]);

    // A root above the first octave of the table is folded back into it.
    let wide = Tuning {
        cents: vec![0, 200, 400, 500, 700, 900, 1100],
        period: 600,
        ..twelve_tet()
    };
    let folded = parse_roman("I", &system("test.seven", &[0, 1, 2, 3, 4, 5, 6]), &wide).unwrap();
    assert_eq!(folded.tones[0], Pitch { cents: 0 });
}

#[test]
fn chords_serialize() {
    let chord = chord_in(&major(), "V7/3");
    let json = serde_json::to_string(&chord).unwrap();
    assert_eq!(serde_json::from_str::<Chord>(&json).unwrap(), chord);
}
