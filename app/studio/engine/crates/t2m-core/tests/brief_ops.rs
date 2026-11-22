mod common;

use std::collections::BTreeSet;
use std::sync::OnceLock;

use proptest::prelude::*;
use t2m_core::brief::{
    Articulation, Assumption, Brief, HarmonicLanguage, Identity, Length, MeterSpec, Motif,
    Register, Role, Section, SectionHarmony, TimeSpec, Tonality, TonicSpec, TrackSpec, Transition,
};
use t2m_core::brief_ops::{BriefIssue, IssueKind, repair, validate};
use t2m_core::{Knowledge, Meter, parse_pitch_class, parse_roman};

fn kb() -> &'static Knowledge {
    static KB: OnceLock<Knowledge> = OnceLock::new();
    KB.get_or_init(common::extended_kb)
}

/// The gallery's starter brief: valid as it stands.
fn starter() -> Brief {
    kb().gallery()[0].brief.clone()
}

fn track(id: &str, role: Role, instrument: &str) -> TrackSpec {
    TrackSpec {
        id: id.into(),
        role,
        instrument: instrument.into(),
        register: Register::Mid,
        pattern: String::new(),
        busy: 0.5,
        articulation: Articulation::Normal,
    }
}

fn section(id: &str, bars: u16) -> Section {
    Section {
        id: id.into(),
        kind: "verse".into(),
        bars,
        energy: 0.5,
        density: 0.5,
        harmony: SectionHarmony::Named {
            id: "prog.axis".into(),
        },
        tracks: vec!["lead".into()],
        transition: Transition::None,
    }
}

fn issues_of(brief: &Brief) -> Vec<BriefIssue> {
    validate(brief, kb()).err().unwrap_or_default()
}

/// Asserts that exactly one rule is broken, and that repair fixes exactly that.
fn one_issue(brief: &Brief, path: &str, kind: IssueKind, was: &str, now: Option<&str>) -> Brief {
    let found = issues_of(brief);
    let expected = BriefIssue {
        path: path.into(),
        kind,
        was: was.into(),
        now: now.map(str::to_string),
    };
    assert_eq!(found, std::slice::from_ref(&expected), "validate");
    let (fixed, repaired) = repair(brief.clone(), kb());
    assert_eq!(repaired, [expected], "repair");
    assert_eq!(
        validate(&fixed, kb()),
        Ok(()),
        "the repaired brief must be valid"
    );
    fixed
}

fn with(edit: impl FnOnce(&mut Brief)) -> Brief {
    let mut brief = starter();
    edit(&mut brief);
    brief
}

#[test]
fn starter_brief_is_valid_and_untouched() {
    assert_eq!(validate(&starter(), kb()), Ok(()));
    assert_eq!(repair(starter(), kb()), (starter(), vec![]));
    for entry in Knowledge::load().unwrap().gallery() {
        assert_eq!(
            validate(&entry.brief, &Knowledge::load().unwrap()),
            Ok(()),
            "{}",
            entry.id
        );
    }
}

#[test]
fn t7_1_t7_2_each_broken_rule_is_one_issue_and_one_repair() {
    use IssueKind::*;
    let b = with(|b| b.identity.style = "nope.zzz".into());
    let fixed = one_issue(
        &b,
        "/identity/style",
        UnknownId,
        "nope.zzz",
        Some("pop.generic"),
    );
    assert_eq!(fixed.identity.style, "pop.generic");

    for (tempo, now) in [
        (20, "30"),
        (29, "30"),
        (400, "300"),
        (301, "300"),
        (0, "30"),
        (u16::MAX, "300"),
    ] {
        let b = with(|b| b.time.tempo_bpm = tempo);
        let fixed = one_issue(
            &b,
            "/time/tempo_bpm",
            OutOfRange,
            &tempo.to_string(),
            Some(now),
        );
        assert_eq!(fixed.time.tempo_bpm.to_string(), now);
    }
    for tempo in [30, 300] {
        assert_eq!(issues_of(&with(|b| b.time.tempo_bpm = tempo)), []);
    }

    let b = with(|b| b.tracks.clear());
    let fixed = one_issue(
        &b,
        "/tracks",
        Missing,
        "0",
        Some("4 tracks from style pop.generic"),
    );
    let ids: Vec<&str> = fixed.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["lead", "chords", "bass", "drums"]);
    assert_eq!(fixed.tracks[2].instrument, "bass.electric");
    assert_eq!(fixed.tracks[0].register, Register::MidHigh);
    assert_eq!(fixed.tracks[0].busy, 0.5);

    let b = with(|b| {
        for n in 0..13 {
            b.tracks
                .push(track(&format!("extra{n}"), Role::Pad, "keys.piano"));
        }
    });
    assert_eq!(b.tracks.len(), 17);
    let fixed = one_issue(&b, "/tracks", TooMany, "17", Some("16"));
    assert_eq!(fixed.tracks.len(), 16);
    assert_eq!(fixed.tracks[15].id, "extra11");
    assert_eq!(
        issues_of(&with(|b| {
            for n in 0..12 {
                b.tracks
                    .push(track(&format!("extra{n}"), Role::Pad, "keys.piano"));
            }
        })),
        []
    );

    let b = with(|b| {
        b.form = (0..25).map(|n| section(&format!("s{n}"), 1)).collect();
        b.time.length = Length::Bars(24);
    });
    let fixed = one_issue(&b, "/form", TooMany, "25", Some("24"));
    assert_eq!(fixed.form.len(), 24);
    assert_eq!(
        issues_of(&with(|b| {
            b.form = (0..24).map(|n| section(&format!("s{n}"), 1)).collect();
        })),
        []
    );

    let b = with(|b| b.form = vec![section("a", 300), section("b", 300)]);
    let fixed = one_issue(&b, "/form", TooMany, "600 bars", Some("512 bars"));
    assert_eq!((fixed.form[0].bars, fixed.form[1].bars), (300, 212));
    let b = with(|b| b.form = vec![section("a", 512), section("b", 1), section("c", 1)]);
    let fixed = one_issue(&b, "/form", TooMany, "514 bars", Some("512 bars"));
    assert_eq!(fixed.form.len(), 1);
    assert_eq!(
        issues_of(&with(|b| b.form = vec![section("a", 511), section("b", 1)])),
        []
    );

    let b = with(|b| b.time.length = Length::Bars(600));
    let fixed = one_issue(&b, "/time/length", OutOfRange, "600", Some("512"));
    assert_eq!(fixed.time.length, Length::Bars(512));
    assert_eq!(issues_of(&with(|b| b.time.length = Length::Bars(512))), []);

    let b = with(|b| b.tracks[0].busy = 1.5);
    one_issue(&b, "/tracks/0/busy", OutOfRange, "1.5", Some("1"));

    let b = with(|b| b.tonality.tuning = "5tet".into());
    one_issue(&b, "/tonality/tuning", Conflict, "5tet", Some("12tet"));
    let b = with(|b| b.tonality.tuning = "zzz".into());
    one_issue(&b, "/tonality/tuning", UnknownId, "zzz", Some("12tet"));

    let b = with(|b| {
        b.tonality.harmonic_language = HarmonicLanguage::Drone;
        b.form.truncate(1);
        b.time.length = Length::Bars(4);
    });
    let fixed = one_issue(&b, "/form/0/harmony", Conflict, "named", Some("drone"));
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Drone {
            degrees: vec![1, 5]
        }
    );
    let b = with(|b| {
        b.tonality.harmonic_language = HarmonicLanguage::None;
        b.form.truncate(1);
        b.form[0].harmony = SectionHarmony::Progression {
            roman: vec!["I".into()],
            bars_per_chord: 1,
        };
    });
    let fixed = one_issue(&b, "/form/0/harmony", Conflict, "progression", Some("none"));
    assert_eq!(fixed.form[0].harmony, SectionHarmony::None);

    let b = with(|b| b.tracks[3].instrument = "keys.piano".into());
    let fixed = one_issue(
        &b,
        "/tracks/3/instrument",
        Conflict,
        "keys.piano",
        Some("kit.standard"),
    );
    assert_eq!(fixed.tracks[3].instrument, "kit.standard");
    let b = with(|b| b.tracks[2].instrument = "kit.standard".into());
    one_issue(
        &b,
        "/tracks/2/instrument",
        Conflict,
        "kit.standard",
        Some("bass.electric"),
    );

    let b = with(|b| b.form[1].tracks.push("ghost".into()));
    let fixed = one_issue(&b, "/form/1/tracks", UnknownId, "ghost", None);
    assert_eq!(fixed.form[1].tracks, ["lead", "chords", "bass", "drums"]);
    let b = with(|b| b.form[1].tracks.push("lead".into()));
    let fixed = one_issue(&b, "/form/1/tracks", Duplicate, "lead", None);
    assert_eq!(fixed.form[1].tracks, ["lead", "chords", "bass", "drums"]);

    let b = with(|b| {
        b.tracks[1].id = "lead".into();
        for section in &mut b.form {
            section.tracks.retain(|id| id != "chords");
        }
    });
    let fixed = one_issue(&b, "/tracks/1/id", Duplicate, "lead", Some("lead_2"));
    assert_eq!(fixed.tracks[1].id, "lead_2");

    let b = with(|b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: Some(vec![2, 2, 2]),
        };
    });
    let fixed = one_issue(
        &b,
        "/time/meter/grouping",
        Conflict,
        "[2, 2, 2]",
        Some("[2, 2, 3]"),
    );
    assert_eq!(
        fixed.time.meter,
        MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None
        }
    );
    assert_eq!(
        issues_of(&with(|b| {
            b.time.meter = MeterSpec::Meter {
                numerator: 7,
                denominator: 8,
                grouping: Some(vec![3, 2, 2]),
            };
        })),
        []
    );

    for version in [0, 2, u32::MAX] {
        let b = with(|b| b.version = version);
        one_issue(&b, "/version", OutOfRange, &version.to_string(), Some("1"));
    }
}

#[test]
fn t7_2_more_rules_each_with_its_repair() {
    use IssueKind::*;
    let b = with(|b| b.identity.fusion = Some("lofi hiphop".into()));
    let fixed = one_issue(
        &b,
        "/identity/fusion",
        UnknownId,
        "lofi hiphop",
        Some("hiphop.lofi"),
    );
    assert_eq!(fixed.identity.fusion.as_deref(), Some("hiphop.lofi"));
    let b = with(|b| b.identity.fusion = Some("qqqqzzzz".into()));
    let fixed = one_issue(&b, "/identity/fusion", UnknownId, "qqqqzzzz", None);
    assert_eq!(fixed.identity.fusion, None);
    assert_eq!(
        issues_of(&with(|b| b.identity.fusion = Some("hiphop.lofi".into()))),
        []
    );

    for (numerator, denominator) in [(0, 4), (4, 0), (4, 3), (4, 64)] {
        let b = with(|b| {
            b.time.meter = MeterSpec::Meter {
                numerator,
                denominator,
                grouping: Some(vec![9]),
            };
        });
        let was = format!("{numerator}/{denominator}");
        let fixed = one_issue(&b, "/time/meter", OutOfRange, &was, Some("4/4"));
        assert_eq!(
            fixed.time.meter,
            MeterSpec::Meter {
                numerator: 4,
                denominator: 4,
                grouping: None
            }
        );
    }

    let b = with(|b| b.time.feel = "lazy swing".into());
    one_issue(
        &b,
        "/time/feel",
        UnknownId,
        "lazy swing",
        Some("groove.lofi_swing"),
    );
    let b = with(|b| b.time.feel = "qqqqzzzz".into());
    one_issue(
        &b,
        "/time/feel",
        UnknownId,
        "qqqqzzzz",
        Some("groove.straight_8"),
    );

    let b = with(|b| b.tonality.system = "Dorian".into());
    one_issue(
        &b,
        "/tonality/system",
        UnknownId,
        "Dorian",
        Some("mode.dorian"),
    );
    let b = with(|b| b.tonality.system = "qqqqzzzz".into());
    one_issue(
        &b,
        "/tonality/system",
        UnknownId,
        "qqqqzzzz",
        Some("mode.major"),
    );
    // An alternative tuning the system lists is accepted; a near name resolves to it.
    assert_eq!(
        issues_of(&with(|b| {
            b.tonality.system = "mode.phrygian".into();
            b.tonality.tuning = "12well".into();
        })),
        []
    );
    let b = with(|b| {
        b.tonality.system = "mode.phrygian".into();
        b.tonality.tuning = "12 well".into();
    });
    one_issue(&b, "/tonality/tuning", UnknownId, "12 well", Some("12well"));
    // A near name that resolves to a tuning the system does not list falls back to the default.
    let b = with(|b| b.tonality.tuning = "12 well".into());
    one_issue(&b, "/tonality/tuning", UnknownId, "12 well", Some("12tet"));

    let b = with(|b| b.tonality.tonic = TonicSpec::Name("H".into()));
    let fixed = one_issue(&b, "/tonality/tonic", Unreadable, "H", Some("C"));
    assert_eq!(fixed.tonality.tonic, TonicSpec::Name("C".into()));
    for name in ["Db", "f#", "E♭"] {
        assert_eq!(
            issues_of(&with(|b| b.tonality.tonic = TonicSpec::Name(name.into()))),
            []
        );
    }
    for hz in [20.0, 146.83, 5000.0] {
        assert_eq!(
            issues_of(&with(|b| b.tonality.tonic = TonicSpec::Hz { hz })),
            [],
            "{hz}"
        );
    }
    for hz in [19.99, 5000.01, 0.0, -440.0] {
        let b = with(|b| b.tonality.tonic = TonicSpec::Hz { hz });
        one_issue(
            &b,
            "/tonality/tonic",
            Unreadable,
            &format!("{hz} Hz"),
            Some("C"),
        );
    }

    let b = with(|b| b.tracks[0].instrument = "Grand Piano".into());
    one_issue(
        &b,
        "/tracks/0/instrument",
        UnknownId,
        "Grand Piano",
        Some("keys.piano"),
    );
    // Unknown, and nothing alike: the style's instrument for the role.
    let b = with(|b| b.tracks[2].instrument = "qqqqzzzz".into());
    one_issue(
        &b,
        "/tracks/2/instrument",
        UnknownId,
        "qqqqzzzz",
        Some("bass.electric"),
    );
    // Unknown, nearest is a kit, but the role is pitched: the style's instrument for the role.
    let b = with(|b| b.tracks[0].instrument = "drum kit".into());
    one_issue(
        &b,
        "/tracks/0/instrument",
        UnknownId,
        "drum kit",
        Some("keys.piano"),
    );
    // A role the style's palette lacks: the first instrument of the right kind.
    let b = with(|b| {
        b.tracks.push(track("perc", Role::Percussion, "qqqqzzzz"));
        b.tracks.push(track("pad", Role::Pad, "qqqqzzzz"));
    });
    let (fixed, issues) = repair(b, kb());
    assert_eq!(issues.len(), 2);
    assert_eq!(fixed.tracks[4].instrument, "kit.standard");
    assert_eq!(fixed.tracks[5].instrument, "bass.electric");
    assert_eq!(
        issues_of(&with(|b| b.tracks.push(track(
            "perc",
            Role::Percussion,
            "kit.tabla"
        )))),
        []
    );

    for (busy, now) in [(-0.1, "0"), (f64::INFINITY, "1"), (f64::NEG_INFINITY, "0")] {
        let b = with(|b| b.tracks[1].busy = busy);
        one_issue(
            &b,
            "/tracks/1/busy",
            OutOfRange,
            &busy.to_string(),
            Some(now),
        );
    }
    for busy in [0.0, 1.0] {
        assert_eq!(issues_of(&with(|b| b.tracks[1].busy = busy)), []);
    }

    let b = with(|b| {
        b.tracks[2].id = String::new();
        for section in &mut b.form {
            section.tracks.retain(|id| id != "bass");
        }
    });
    let fixed = one_issue(&b, "/tracks/2/id", Missing, "", Some("track3"));
    assert_eq!(fixed.tracks[2].id, "track3");

    let b = with(|b| b.form[2].id = "intro".into());
    one_issue(&b, "/form/2/id", Duplicate, "intro", Some("intro_2"));
    let b = with(|b| b.form[0].id = String::new());
    one_issue(&b, "/form/0/id", Missing, "", Some("section1"));
    let b = with(|b| b.form[0].bars = 0);
    let fixed = one_issue(&b, "/form/0/bars", OutOfRange, "0", Some("1"));
    assert_eq!(fixed.form[0].bars, 1);
    let b = with(|b| b.form[3].energy = -1.0);
    one_issue(&b, "/form/3/energy", OutOfRange, "-1", Some("0"));
    let b = with(|b| b.form[3].density = 7.0);
    one_issue(&b, "/form/3/density", OutOfRange, "7", Some("1"));

    let named = |id: &str| SectionHarmony::Named { id: id.into() };
    let b = with(|b| b.form[0].harmony = named("andalusian cadence"));
    let fixed = one_issue(
        &b,
        "/form/0/harmony/id",
        UnknownId,
        "andalusian cadence",
        Some("prog.andalusian"),
    );
    assert_eq!(fixed.form[0].harmony, named("prog.andalusian"));
    let b = with(|b| b.form[0].harmony = named("qqqqzzzz"));
    one_issue(
        &b,
        "/form/0/harmony/id",
        UnknownId,
        "qqqqzzzz",
        Some("prog.axis"),
    );

    let progression = |roman: &[&str], bars_per_chord| SectionHarmony::Progression {
        roman: roman.iter().map(|s| s.to_string()).collect(),
        bars_per_chord,
    };
    let b = with(|b| b.form[0].harmony = progression(&["I", "V77", "vi"], 1));
    let fixed = one_issue(&b, "/form/0/harmony/roman/1", Unreadable, "V77", Some("I"));
    assert_eq!(fixed.form[0].harmony, progression(&["I", "I", "vi"], 1));
    let b = with(|b| b.form[0].harmony = progression(&["I"], 0));
    one_issue(
        &b,
        "/form/0/harmony/bars_per_chord",
        OutOfRange,
        "0",
        Some("1"),
    );
    let b = with(|b| b.form[0].harmony = progression(&[], 2));
    let fixed = one_issue(&b, "/form/0/harmony/roman", Missing, "[]", Some("I"));
    assert_eq!(fixed.form[0].harmony, progression(&["I"], 2));
    let many: Vec<&str> = std::iter::repeat_n("I", 65).collect();
    let b = with(|b| b.form[0].harmony = progression(&many, 1));
    one_issue(&b, "/form/0/harmony/roman", TooMany, "65", Some("64"));
    assert_eq!(
        issues_of(&with(|b| b.form[0].harmony = progression(&many[..64], 1))),
        []
    );
    // In a minor mode the stand-in chord is the minor tonic.
    let b = with(|b| {
        b.tonality.system = "mode.minor".into();
        b.form[0].harmony = progression(&["oops"], 1);
    });
    one_issue(&b, "/form/0/harmony/roman/0", Unreadable, "oops", Some("i"));
    let long = "V".repeat(500);
    let b = with(|b| b.form[0].harmony = progression(&[&long], 1));
    one_issue(
        &b,
        "/form/0/harmony/roman/0",
        Unreadable,
        &"V".repeat(32),
        Some("I"),
    );

    let drone = |degrees: &[u8]| SectionHarmony::Drone {
        degrees: degrees.to_vec(),
    };
    for ok in [&[1u8, 5][..], &[1], &[7, 1]] {
        assert_eq!(
            issues_of(&with(|b| b.form[0].harmony = drone(ok))),
            [],
            "{ok:?}"
        );
    }
    for (bad, now, shown) in [
        (&[][..], &[1u8, 5][..], "[1, 5]"),
        (&[0, 5], &[5], "[5]"),
        (&[8], &[1], "[1]"),
        (&[5, 5, 1], &[5, 1], "[5, 1]"),
    ] {
        let b = with(|b| b.form[0].harmony = drone(bad));
        let fixed = one_issue(
            &b,
            "/form/0/harmony/degrees",
            OutOfRange,
            &format!("{bad:?}"),
            Some(shown),
        );
        assert_eq!(fixed.form[0].harmony, drone(now));
    }

    // Length: at least a bar per section, at least one bar or second.
    let b = with(|b| b.time.length = Length::Bars(3));
    one_issue(&b, "/time/length", OutOfRange, "3", Some("4"));
    assert_eq!(issues_of(&with(|b| b.time.length = Length::Cycles(4))), []);
    let b = with(|b| b.time.length = Length::Cycles(0));
    one_issue(&b, "/time/length", OutOfRange, "0", Some("4"));
    let b = with(|b| b.time.length = Length::Seconds(0));
    one_issue(&b, "/time/length", OutOfRange, "0 s", Some("1 s"));
    // 512 bars of 4/4 at 116 bpm last 1059.3 seconds.
    assert_eq!(
        issues_of(&with(|b| b.time.length = Length::Seconds(1059))),
        []
    );
    let b = with(|b| b.time.length = Length::Seconds(1060));
    one_issue(&b, "/time/length", OutOfRange, "1060 s", Some("1059 s"));
    // In 7/8 the beat limit (2,048 beats = 292 bars) applies, so the cap is shorter.
    let b = with(|b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None,
        };
        b.time.length = Length::Seconds(1000);
    });
    one_issue(&b, "/time/length", OutOfRange, "1000 s", Some("528 s"));
    // A very fast, very short bar still allows at least one second.
    let b = with(|b| {
        b.time.tempo_bpm = 300;
        b.time.meter = MeterSpec::Meter {
            numerator: 1,
            denominator: 32,
            grouping: None,
        };
        b.time.length = Length::Seconds(60);
    });
    one_issue(&b, "/time/length", OutOfRange, "60 s", Some("12 s"));

    let b = with(|b| {
        b.assumptions = vec![Assumption {
            field: "/x".into(),
            because: "y".into(),
            confidence: 3.0,
        }]
    });
    one_issue(&b, "/assumptions/0/confidence", OutOfRange, "3", Some("1"));
}

#[test]
fn t7_2_motif_rules() {
    use IssueKind::*;
    let motif = |degrees: &[i32], durations: &[f64]| {
        with(|b| {
            b.motif = Some(Motif {
                degrees: degrees.to_vec(),
                durations: durations.to_vec(),
                development: "sequence".into(),
            });
        })
    };
    assert_eq!(
        issues_of(&motif(&[0, 2, 4, -21, 21], &[1.0, 0.5, 0.25, 16.0, 0.001])),
        []
    );
    let fixed = one_issue(
        &motif(&[0, 2, 4], &[1.0, 1.0]),
        "/motif",
        Conflict,
        "3 degrees, 2 durations",
        Some("2 of each"),
    );
    assert_eq!(fixed.motif.unwrap().degrees, [0, 2]);
    let fixed = one_issue(
        &motif(&[0], &[1.0, 1.0]),
        "/motif",
        Conflict,
        "1 degrees, 2 durations",
        Some("1 of each"),
    );
    assert_eq!(fixed.motif.unwrap().durations, [1.0]);
    let fixed = one_issue(&motif(&[], &[]), "/motif", Missing, "no notes", None);
    assert_eq!(fixed.motif, None);
    let many = vec![0; 33];
    let fixed = one_issue(
        &motif(&many, &vec![1.0; 33]),
        "/motif",
        TooMany,
        "33",
        Some("32"),
    );
    assert_eq!(fixed.motif.as_ref().unwrap().degrees.len(), 32);
    assert_eq!(fixed.motif.unwrap().durations.len(), 32);
    assert_eq!(issues_of(&motif(&many[..32], &[1.0; 32])), []);
    one_issue(
        &motif(&[0, 22], &[1.0, 1.0]),
        "/motif/degrees/1",
        OutOfRange,
        "22",
        Some("21"),
    );
    one_issue(
        &motif(&[-22], &[1.0]),
        "/motif/degrees/0",
        OutOfRange,
        "-22",
        Some("-21"),
    );
    one_issue(
        &motif(&[0, 0], &[1.0, 16.5]),
        "/motif/durations/1",
        OutOfRange,
        "16.5",
        Some("16"),
    );
    for bad in [0.0, -1.0, f64::INFINITY, f64::NEG_INFINITY] {
        one_issue(
            &motif(&[0], &[bad]),
            "/motif/durations/0",
            OutOfRange,
            &bad.to_string(),
            Some("1"),
        );
    }
    // Lengths differ and nothing is left.
    let (fixed, issues) = repair(motif(&[], &[1.0]), kb());
    assert_eq!(fixed.motif, None);
    assert_eq!(issues.len(), 2);
}

#[test]
fn t7_2_cycles_need_a_groove_that_plays_them() {
    use IssueKind::*;
    let cycle = |id: &str, feel: &str| {
        with(|b| {
            b.time.meter = MeterSpec::Cycle { id: id.into() };
            b.time.feel = feel.into();
        })
    };
    assert_eq!(issues_of(&cycle("tala.rupak", "groove.rupak")), []);
    let fixed = one_issue(
        &cycle("tala.rupak", "groove.straight_8"),
        "/time/feel",
        Conflict,
        "groove.straight_8",
        Some("groove.rupak"),
    );
    assert_eq!(fixed.time.feel, "groove.rupak");
    let fixed = one_issue(
        &cycle("rupak", "groove.rupak"),
        "/time/meter/id",
        UnknownId,
        "rupak",
        Some("tala.rupak"),
    );
    assert_eq!(
        fixed.time.meter,
        MeterSpec::Cycle {
            id: "tala.rupak".into()
        }
    );
    // No groove plays teental: it is kept as sixteen beats grouped in fours.
    let fixed = one_issue(
        &cycle("tala.teental", "groove.straight_8"),
        "/time/meter",
        Conflict,
        "tala.teental",
        Some("16/4 grouped [4, 4, 4, 4]"),
    );
    assert_eq!(
        fixed.time.meter,
        MeterSpec::Meter {
            numerator: 16,
            denominator: 4,
            grouping: Some(vec![4, 4, 4, 4])
        }
    );
    // An unknown cycle with nothing alike falls back to the style's meter.
    let fixed = one_issue(
        &cycle("qqqqzzzz", "groove.straight_8"),
        "/time/meter",
        UnknownId,
        "qqqqzzzz",
        Some("4/4"),
    );
    assert_eq!(
        fixed.time.meter,
        MeterSpec::Meter {
            numerator: 4,
            denominator: 4,
            grouping: None
        }
    );
    // Seconds are measured against the cycle's length: 512 cycles of 7 beats at 116 bpm.
    let b = with(|b| {
        b.time.meter = MeterSpec::Cycle {
            id: "tala.rupak".into(),
        };
        b.time.feel = "groove.rupak".into();
        b.time.length = Length::Seconds(2000);
    });
    one_issue(&b, "/time/length", OutOfRange, "2000 s", Some("1057 s"));
}

#[test]
fn t7_2_systems_without_chords_use_a_drone() {
    let b = with(|b| {
        b.tonality.system = "raga.test".into();
        b.form.truncate(2);
        b.time.length = Length::Bars(12);
    });
    let (fixed, issues) = repair(b, kb());
    let paths: Vec<&str> = issues.iter().map(|issue| issue.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/tonality/harmonic_language",
            "/form/0/harmony",
            "/form/1/harmony"
        ]
    );
    assert_eq!(issues[0].kind, IssueKind::Conflict);
    assert_eq!(
        (issues[0].was.as_str(), issues[0].now.as_deref()),
        ("functional", Some("drone"))
    );
    assert_eq!(fixed.tonality.harmonic_language, HarmonicLanguage::Drone);
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Drone {
            degrees: vec![1, 5]
        }
    );
    assert_eq!(validate(&fixed, kb()), Ok(()));
    let modal = with(|b| {
        b.tonality.system = "raga.test".into();
        b.tonality.harmonic_language = HarmonicLanguage::Modal;
    });
    assert_eq!(issues_of(&modal)[0].was, "modal");

    // Drone and none are accepted as given, on any system; drone sections may sit in a chordal piece.
    for language in [HarmonicLanguage::Drone, HarmonicLanguage::None] {
        let b = with(|b| {
            b.tonality.system = "raga.test".into();
            b.tonality.harmonic_language = language;
            for section in &mut b.form {
                section.harmony = SectionHarmony::None;
            }
        });
        assert_eq!(issues_of(&b), []);
    }
    assert_eq!(
        issues_of(&with(|b| b.form[0].harmony = SectionHarmony::None)),
        []
    );

    // A five-degree system has no fifth degree to drone on beyond its own five.
    let b = with(|b| {
        b.tonality.system = "scale.five".into();
        b.tonality.tuning = "5tet".into();
        b.tonality.harmonic_language = HarmonicLanguage::Drone;
        for section in &mut b.form {
            section.harmony = SectionHarmony::Drone {
                degrees: vec![1, 5, 6],
            };
        }
    });
    let (fixed, issues) = repair(b, kb());
    assert_eq!(issues.len(), 4);
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Drone {
            degrees: vec![1, 5]
        }
    );
}

#[test]
fn t7_3_named_repairs() {
    let b = with(|b| b.identity.style = "lofi hiphop".into());
    let (fixed, issues) = repair(b, kb());
    assert_eq!(fixed.identity.style, "hiphop.lofi");
    assert_eq!(issues.len(), 1);

    let (fixed, _) = repair(with(|b| b.time.tempo_bpm = 400), kb());
    assert_eq!(fixed.time.tempo_bpm, 300);

    let b = with(|b| {
        for n in 0..13 {
            b.tracks
                .push(track(&format!("t{n}"), Role::Pad, "keys.piano"));
        }
    });
    let first: Vec<String> = b.tracks.iter().take(16).map(|t| t.id.clone()).collect();
    let (fixed, _) = repair(b, kb());
    let kept: Vec<String> = fixed.tracks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(kept, first);

    // An empty form becomes the style's default form, with each section's roles applied.
    let b = with(|b| {
        b.form.clear();
        b.time.length = Length::Bars(52);
    });
    let (fixed, issues) = repair(b, kb());
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (issues[0].path.as_str(), issues[0].kind),
        ("/form", IssueKind::Missing)
    );
    assert_eq!(
        issues[0].now.as_deref(),
        Some("8 sections from style pop.generic")
    );
    let ids: Vec<&str> = fixed.form.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "intro", "verse1", "chorus1", "verse2", "chorus2", "bridge", "chorus3", "outro"
        ]
    );
    assert_eq!(fixed.form[0].tracks, ["chords", "drums"]);
    assert_eq!(fixed.form[1].tracks, ["lead", "chords", "bass", "drums"]);
    assert_eq!(fixed.form[7].tracks, ["lead", "chords"]);
    assert_eq!(fixed.form[2].transition, Transition::FillLight);
    assert_eq!(
        (
            fixed.form[2].bars,
            fixed.form[2].energy,
            fixed.form[2].kind.as_str()
        ),
        (8, 0.85, "chorus")
    );
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Named {
            id: "prog.axis".into()
        }
    );

    let (fixed, _) = repair(
        with(|b| b.tonality.tonic = TonicSpec::Name("H".into())),
        kb(),
    );
    assert_eq!(fixed.tonality.tonic, TonicSpec::Name("C".into()));

    // The default form follows the piece's harmonic language.
    let b = with(|b| {
        b.identity.style = "raga.style".into();
        b.tonality.system = "raga.test".into();
        b.tonality.harmonic_language = HarmonicLanguage::Drone;
        b.time.meter = MeterSpec::Cycle {
            id: "tala.rupak".into(),
        };
        b.time.feel = "groove.rupak".into();
        b.tracks.clear();
        b.form.clear();
    });
    let (fixed, issues) = repair(b, kb());
    assert_eq!(validate(&fixed, kb()), Ok(()));
    let ids: Vec<&str> = fixed.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["lead", "drone", "percussion", "lead_2"]);
    assert_eq!(fixed.tracks[2].instrument, "kit.tabla");
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Drone {
            degrees: vec![1, 5]
        }
    );
    assert_eq!(fixed.form[0].tracks, ["lead", "drone", "lead_2"]);
    assert_eq!(issues.len(), 3, "{issues:?}");
    let none = with(|b| {
        b.tonality.harmonic_language = HarmonicLanguage::None;
        b.form.clear();
    });
    assert_eq!(repair(none, kb()).0.form[0].harmony, SectionHarmony::None);
    // A style with no progressions of its own gets the tonic chord.
    let b = with(|b| {
        b.identity.style = "raga.style".into();
        b.form.clear();
        b.time.length = Length::Bars(12);
    });
    let (fixed, _) = repair(b, kb());
    assert_eq!(
        fixed.form[0].harmony,
        SectionHarmony::Progression {
            roman: vec!["I".into()],
            bars_per_chord: 1
        }
    );
}

#[test]
fn t7_6_scenario_s1_misspelt_style_fast_tempo_too_many_tracks() {
    let mut brief = starter();
    brief.identity.style = "popp".into();
    brief.time.tempo_bpm = 400;
    for n in 0..13 {
        brief
            .tracks
            .push(track(&format!("extra{n}"), Role::Pad, "keys.piano"));
    }
    assert!(validate(&brief, kb()).is_err());
    let (fixed, issues) = repair(brief, kb());
    assert_eq!(validate(&fixed, kb()), Ok(()));
    let listed: Vec<(&str, &str, Option<&str>)> = issues
        .iter()
        .map(|issue| {
            (
                issue.path.as_str(),
                issue.was.as_str(),
                issue.now.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("/identity/style", "popp", Some("pop.generic")),
            ("/time/tempo_bpm", "400", Some("300")),
            ("/tracks", "17", Some("16")),
        ]
    );
    let json = serde_json::to_string(&issues[0]).unwrap();
    assert_eq!(
        json,
        r#"{"path":"/identity/style","kind":"unknown_id","was":"popp","now":"pop.generic"}"#
    );
}

#[test]
fn t7_7_non_finite_controls_are_brought_into_range() {
    let nan = f64::NAN;
    let mut brief = starter();
    brief.tracks[0].busy = nan;
    brief.form[0].energy = nan;
    brief.form[0].density = f64::INFINITY;
    brief.form[1].energy = f64::NEG_INFINITY;
    brief.form[1].density = -3.0;
    brief.tonality.tonic = TonicSpec::Hz { hz: nan };
    brief.motif = Some(Motif {
        degrees: vec![0, 1],
        durations: vec![nan, f64::INFINITY],
        development: String::new(),
    });
    brief.assumptions = vec![Assumption {
        field: "/x".into(),
        because: "y".into(),
        confidence: nan,
    }];
    let (fixed, issues) = repair(brief, kb());
    assert_eq!(validate(&fixed, kb()), Ok(()));
    assert_eq!(issues.len(), 9, "{issues:?}");
    assert!(issues.iter().all(|issue| issue.now.is_some()));
    assert_eq!(fixed.tracks[0].busy, 0.5);
    assert_eq!((fixed.form[0].energy, fixed.form[0].density), (0.5, 1.0));
    assert_eq!((fixed.form[1].energy, fixed.form[1].density), (0.0, 0.0));
    assert_eq!(fixed.tonality.tonic, TonicSpec::Name("C".into()));
    assert_eq!(fixed.motif.unwrap().durations, [1.0, 1.0]);
    assert_eq!(fixed.assumptions[0].confidence, 0.5);
    for hz in [f64::INFINITY, f64::NEG_INFINITY] {
        let (fixed, _) = repair(with(|b| b.tonality.tonic = TonicSpec::Hz { hz }), kb());
        assert_eq!(fixed.tonality.tonic, TonicSpec::Name("C".into()));
    }
}

/// The documented limits, checked without using `validate`.
fn assert_shape(brief: &Brief) {
    let kb = kb();
    assert_eq!(brief.version, 1);
    assert!(kb.style(&brief.identity.style).is_some());
    assert!(
        brief
            .identity
            .fusion
            .as_ref()
            .is_none_or(|id| kb.style(id).is_some())
    );
    assert!((30..=300).contains(&brief.time.tempo_bpm));
    let groove = kb.groove(&brief.time.feel).expect("known groove");
    match &brief.time.meter {
        MeterSpec::Meter {
            numerator,
            denominator,
            grouping,
        } => {
            Meter::new(*numerator, *denominator, grouping.clone()).expect("usable meter");
        }
        MeterSpec::Cycle { id } => {
            assert!(kb.cycle(id).is_some());
            assert!(groove.cycles.contains(id));
        }
    }
    let system = kb
        .tonal_system(&brief.tonality.system)
        .expect("known system");
    let tuning = kb.tuning(&brief.tonality.tuning).expect("known tuning");
    assert!(system.default_tuning == tuning.id || system.tunings.contains(&tuning.id));
    match &brief.tonality.tonic {
        TonicSpec::Name(name) => assert!(parse_pitch_class(name).is_some()),
        TonicSpec::Hz { hz } => assert!((20.0..=5000.0).contains(hz)),
    }
    let language = brief.tonality.harmonic_language;
    let chordal = matches!(
        language,
        HarmonicLanguage::Functional | HarmonicLanguage::Modal
    );
    assert!(system.chordable || !chordal);

    assert!((1..=16).contains(&brief.tracks.len()));
    let track_ids: BTreeSet<&str> = brief.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(track_ids.len(), brief.tracks.len());
    assert!(!track_ids.contains(""));
    for track in &brief.tracks {
        let instrument = kb.instrument(&track.instrument).expect("known instrument");
        let kit = matches!(track.role, Role::Drums | Role::Percussion);
        assert_eq!(instrument.is_kit(), kit);
        assert!((0.0..=1.0).contains(&track.busy));
    }

    assert!((1..=24).contains(&brief.form.len()));
    let section_ids: BTreeSet<&str> = brief.form.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(section_ids.len(), brief.form.len());
    assert!(!section_ids.contains(""));
    let total: u32 = brief.form.iter().map(|s| u32::from(s.bars)).sum();
    assert!((1..=512).contains(&total));
    for section in &brief.form {
        assert!(section.bars >= 1);
        assert!((0.0..=1.0).contains(&section.energy));
        assert!((0.0..=1.0).contains(&section.density));
        let listed: BTreeSet<&str> = section.tracks.iter().map(String::as_str).collect();
        assert_eq!(listed.len(), section.tracks.len());
        assert!(listed.is_subset(&track_ids));
        match &section.harmony {
            SectionHarmony::None => {}
            SectionHarmony::Drone { degrees } => {
                assert!(!degrees.is_empty());
                assert!(degrees.iter().all(|d| (1..=system.degrees()).contains(d)));
            }
            SectionHarmony::Named { id } => {
                assert!(chordal);
                assert!(kb.progression(id).is_some());
            }
            SectionHarmony::Progression {
                roman,
                bars_per_chord,
            } => {
                assert!(chordal);
                assert!(*bars_per_chord >= 1);
                assert!((1..=64).contains(&roman.len()));
                for symbol in roman {
                    parse_roman(symbol, system, tuning).expect("readable chord");
                }
            }
        }
    }
    match brief.time.length {
        Length::Bars(n) | Length::Cycles(n) => {
            assert!(usize::from(n) >= brief.form.len() && n <= 512);
        }
        Length::Seconds(n) => assert!(n >= 1),
    }
    if let Some(motif) = &brief.motif {
        assert_eq!(motif.degrees.len(), motif.durations.len());
        assert!((1..=32).contains(&motif.degrees.len()));
        assert!(motif.degrees.iter().all(|d| (-21..=21).contains(d)));
        assert!(motif.durations.iter().all(|d| *d > 0.0 && *d <= 16.0));
    }
    assert!(
        brief
            .assumptions
            .iter()
            .all(|a| (0.0..=1.0).contains(&a.confidence))
    );
}

fn any_f64() -> impl Strategy<Value = f64> {
    prop_oneof![proptest::num::f64::ANY, -1.0..2.0f64, Just(0.5)]
}

/// Mostly ids of the wanted kind, sometimes ids of another kind, sometimes noise.
fn any_id(pool: &'static [&'static str]) -> impl Strategy<Value = String> {
    const OTHER: &[&str] = &[
        "pop.generic",
        "kit.standard",
        "keys.piano",
        "12tet",
        "mode.major",
        "tala.rupak",
        "groove.straight_8",
        "prog.axis",
        "",
        "lofi hiphop",
        "piano",
        "drums",
        "dorian",
    ];
    prop_oneof![
        4 => proptest::sample::select(pool).prop_map(str::to_string),
        1 => proptest::sample::select(OTHER).prop_map(str::to_string),
        1 => "[a-z .]{0,12}",
        1 => any::<String>(),
    ]
}

fn small_id() -> impl Strategy<Value = String> {
    proptest::sample::select(&["a", "b", "c", "d", "e", "lead", "drums", ""][..])
        .prop_map(str::to_string)
}

fn any_harmony() -> impl Strategy<Value = SectionHarmony> {
    let symbol = proptest::sample::select(
        &[
            "I", "V", "vi", "IV", "i7", "bVII", "V77", "", "xyz", "V/V", "V/3", "viiø7",
        ][..],
    )
    .prop_map(str::to_string);
    prop_oneof![
        (proptest::collection::vec(symbol, 0..70), any::<u8>()).prop_map(
            |(roman, bars_per_chord)| SectionHarmony::Progression {
                roman,
                bars_per_chord
            }
        ),
        proptest::collection::vec(0u8..10, 0..5)
            .prop_map(|degrees| SectionHarmony::Drone { degrees }),
        any_id(&["prog.axis", "prog.andalusian"]).prop_map(|id| SectionHarmony::Named { id }),
        Just(SectionHarmony::None),
    ]
}

fn any_section() -> impl Strategy<Value = Section> {
    let transition = proptest::sample::select(
        &[
            Transition::None,
            Transition::FillLight,
            Transition::Crash,
            Transition::Riser,
        ][..],
    );
    (
        small_id(),
        prop_oneof![0u16..40, any::<u16>()],
        any_f64(),
        any_f64(),
        any_harmony(),
        proptest::collection::vec(small_id(), 0..8),
        transition,
    )
        .prop_map(
            |(id, bars, energy, density, harmony, tracks, transition)| Section {
                id,
                kind: "x".into(),
                bars,
                energy,
                density,
                harmony,
                tracks,
                transition,
            },
        )
}

fn any_track() -> impl Strategy<Value = TrackSpec> {
    let role = proptest::sample::select(
        &[
            Role::Lead,
            Role::Counter,
            Role::Chords,
            Role::Pad,
            Role::Bass,
            Role::Drums,
            Role::Percussion,
            Role::Arp,
            Role::Drone,
            Role::Fx,
        ][..],
    );
    let instrument = any_id(&[
        "keys.piano",
        "bass.electric",
        "kit.standard",
        "kit.tabla",
        "strings.sitar",
    ]);
    (small_id(), role, instrument, any_f64()).prop_map(|(id, role, instrument, busy)| TrackSpec {
        busy,
        ..track(&id, role, &instrument)
    })
}

fn any_brief() -> impl Strategy<Value = Brief> {
    let meter = prop_oneof![
        (
            any::<u8>(),
            proptest::sample::select(&[0u8, 1, 2, 3, 4, 8, 16, 32, 64][..]),
            proptest::option::of(proptest::collection::vec(0u8..6, 0..5)),
        )
            .prop_map(|(numerator, denominator, grouping)| MeterSpec::Meter {
                numerator,
                denominator,
                grouping
            }),
        any_id(&["tala.rupak", "tala.teental"]).prop_map(|id| MeterSpec::Cycle { id }),
    ];
    let length = prop_oneof![
        any::<u16>().prop_map(Length::Bars),
        any::<u16>().prop_map(Length::Cycles),
        any::<u16>().prop_map(Length::Seconds),
    ];
    let tonic = prop_oneof![
        proptest::sample::select(&["C", "Db", "f#", "H", ""][..])
            .prop_map(|s| TonicSpec::Name(s.into())),
        any_f64().prop_map(|hz| TonicSpec::Hz { hz: hz * 500.0 }),
    ];
    let language = proptest::sample::select(
        &[
            HarmonicLanguage::Functional,
            HarmonicLanguage::Modal,
            HarmonicLanguage::Drone,
            HarmonicLanguage::None,
        ][..],
    );
    let motif = proptest::option::of(
        (
            proptest::collection::vec(prop_oneof![-30i32..30, any::<i32>()], 0..40),
            proptest::collection::vec(any_f64(), 0..40),
        )
            .prop_map(|(degrees, durations)| Motif {
                degrees,
                durations,
                development: String::new(),
            }),
    );
    let identity = (
        any_id(&["pop.generic", "hiphop.lofi", "raga.style"]),
        proptest::option::of(any_id(&["pop.generic", "hiphop.lofi"])),
    );
    let time = (
        prop_oneof![20u16..320, any::<u16>()],
        meter,
        any_id(&["groove.straight_8", "groove.rupak", "groove.lofi_swing"]),
        length,
    );
    let tonality = (
        tonic,
        any_id(&[
            "mode.major",
            "mode.minor",
            "mode.dorian",
            "mode.phrygian",
            "raga.test",
            "scale.five",
        ]),
        any_id(&["12tet", "12well", "5tet"]),
        language,
    );
    (
        prop_oneof![Just(1u32), any::<u32>()],
        identity,
        time,
        tonality,
        proptest::collection::vec(any_section(), 0..60),
        proptest::collection::vec(any_track(), 0..40),
        motif,
        proptest::collection::vec(any_f64(), 0..3),
    )
        .prop_map(
            |(version, identity, time, tonality, form, tracks, motif, confidences)| Brief {
                version,
                identity: Identity {
                    title: String::new(),
                    summary: String::new(),
                    style: identity.0,
                    fusion: identity.1,
                    references: vec![],
                },
                time: TimeSpec {
                    tempo_bpm: time.0,
                    meter: time.1,
                    feel: time.2,
                    length: time.3,
                },
                tonality: Tonality {
                    tonic: tonality.0,
                    system: tonality.1,
                    tuning: tonality.2,
                    harmonic_language: tonality.3,
                },
                form,
                tracks,
                motif,
                assumptions: confidences
                    .into_iter()
                    .map(|confidence| Assumption {
                        field: "/x".into(),
                        because: "y".into(),
                        confidence,
                    })
                    .collect(),
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_500))]

    #[test]
    fn t7_4_any_brief_repairs_to_a_valid_one(brief in any_brief()) {
        let (fixed, _issues) = repair(brief, kb());
        prop_assert_eq!(validate(&fixed, kb()), Ok(()));
        assert_shape(&fixed);
        // A repaired brief survives a trip through JSON unchanged.
        let reread = Brief::from_json(&fixed.to_json());
        prop_assert_eq!(reread, Ok(fixed));
    }

    #[test]
    fn t7_5_repair_leaves_a_valid_brief_alone(brief in any_brief()) {
        let (valid, _) = repair(brief, kb());
        let (again, issues) = repair(valid.clone(), kb());
        prop_assert_eq!(issues, vec![]);
        prop_assert_eq!(again, valid);
    }

    #[test]
    fn t7_4_validate_and_repair_report_the_same_issues(brief in any_brief()) {
        let reported = validate(&brief, kb()).err().unwrap_or_default();
        let (_, repaired) = repair(brief, kb());
        prop_assert_eq!(reported, repaired);
    }
}
