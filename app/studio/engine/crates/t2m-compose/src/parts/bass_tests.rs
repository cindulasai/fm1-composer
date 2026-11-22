use t2m_core::brief::{Brief, Length, Role, SectionHarmony};

use super::*;
use crate::harmony::chord_timeline;
use crate::parts::drums::{drums, kick_onsets};
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, progression, section, track, with};

const FAMILIES: [&str; 7] = [
    "root_hold",
    "root_eighths",
    "kick_lock",
    "walking",
    "octave_bounce",
    "syncopated",
    "tumbao_anticipated",
];

/// One section of drums and bass over `I V vi IV`, a chord a bar.
fn band(pattern: &str, feel: &str, bars: u16) -> Brief {
    with(|b| {
        b.time.feel = feel.into();
        let mut bass = track("bass", Role::Bass, "bass.electric");
        bass.pattern = pattern.into();
        b.tracks = vec![track("drums", Role::Drums, "kit.standard"), bass];
        b.form = vec![section("a", bars, &["drums", "bass"])];
        b.time.length = Length::Bars(bars);
    })
}

struct Played {
    notes: Vec<Note>,
    spans: Vec<ChordSpan>,
    kicks: Vec<Tick>,
    low: i32,
    high: i32,
}

fn play_with(brief: &Brief, seed: u64, narrow: Option<(i32, i32)>) -> Played {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).expect("a valid brief resolves");
    let spans = chord_timeline(&plan).remove(0);
    let section = &plan.sections[0];
    let drum_track = plan.track("drums").unwrap();
    let kicks = kick_onsets(&drums(&plan, drum_track, section), drum_track);
    let found = plan.track("bass").unwrap();
    let (low, high) = narrow.map_or((found.low, found.high), |(l, h)| {
        (plan.cents_of_midi(l), plan.cents_of_midi(h))
    });
    let bass_track = PlanTrack {
        id: found.id,
        role: found.role,
        instrument: found.instrument,
        low,
        high,
        busy: found.busy,
        pattern: found.pattern,
        articulation: found.articulation,
    };
    let notes = bass(&plan, &bass_track, section, &spans, &kicks);
    Played {
        notes,
        spans,
        kicks,
        low,
        high,
    }
}

fn play(brief: &Brief) -> Played {
    play_with(brief, 1, None)
}

fn class(cents: i32) -> i32 {
    cents.rem_euclid(1200)
}

#[test]
fn t12_1_every_chord_starts_on_its_root() {
    for family in FAMILIES {
        let played = play(&band(family, "groove.rock_8", 8));
        assert_eq!(played.spans.len(), 8);
        for (index, span) in played.spans.iter().enumerate() {
            let root = class(span.chord.bass.cents);
            if family == "tumbao_anticipated" && index > 0 {
                // The root arrives an eighth early, on the last eighth of the bar before.
                let early = played.notes.iter().find(|n| n.start == span.start - 480);
                assert_eq!(
                    early.map(|n| class(n.pitch.cents)),
                    Some(root),
                    "{family} {index}"
                );
            } else {
                let first = played.notes.iter().find(|n| n.start == span.start);
                assert_eq!(
                    first.map(|n| class(n.pitch.cents)),
                    Some(root),
                    "{family} {index}"
                );
            }
        }
    }
}

#[test]
fn t12_2_kick_lock_follows_the_kick_drum() {
    for feel in ["groove.boom_bap", "groove.trap", "groove.rock_8"] {
        let played = play(&band("kick_lock", feel, 8));
        assert!(!played.kicks.is_empty());
        let near = |at: Tick| played.kicks.iter().any(|kick| (kick - at).abs() <= 240);
        let locked = played.notes.iter().filter(|n| near(n.start)).count();
        assert!(
            locked * 10 >= played.notes.len() * 8,
            "{feel}: {locked} of {}",
            played.notes.len()
        );
    }
}

#[test]
fn t12_3_walking_plays_every_beat_and_approaches_the_next_root() {
    let played = play(&band("walking", "groove.jazz_ride", 8));
    for (index, span) in played.spans.iter().enumerate() {
        let starts: Vec<Tick> = played
            .notes
            .iter()
            .filter(|n| n.start >= span.start && n.start < span.end)
            .map(|n| n.start)
            .collect();
        assert_eq!(
            starts,
            [0, 1, 2, 3].map(|b| span.start + b * 960),
            "chord {index}"
        );
    }
    for pair in played.spans.windows(2) {
        let last = played
            .notes
            .iter()
            .find(|n| n.start == pair[0].end - 960)
            .unwrap();
        let next = played
            .notes
            .iter()
            .find(|n| n.start == pair[1].start)
            .unwrap();
        let gap = (last.pitch.cents - next.pitch.cents).abs();
        assert!(
            gap == 100 || gap == 200,
            "approach is {gap} cents from the next root"
        );
    }
}

#[test]
fn t12_4_every_note_is_in_register_and_in_the_instruments_range() {
    for family in FAMILIES {
        for narrow in [None, Some((40, 46)), Some((28, 30))] {
            let played = play_with(&band(family, "groove.funk_16", 8), 3, narrow);
            for note in &played.notes {
                assert!(
                    note.pitch.cents >= played.low && note.pitch.cents <= played.high,
                    "{family} {narrow:?}: {}",
                    note.pitch.cents
                );
            }
        }
    }
    // With the usual range that is MIDI 28 to 52.
    let plan_low = {
        let rerolls = Rerolls::default();
        let brief = band("root_eighths", "groove.rock_8", 4);
        let plan = resolve(&brief, kb(), 1, &rerolls).unwrap();
        (plan.cents_of_midi(28), plan.cents_of_midi(52))
    };
    let played = play(&band("octave_bounce", "groove.house", 8));
    assert_eq!((played.low, played.high), plan_low);
    assert!(
        played
            .notes
            .iter()
            .any(|n| n.pitch.cents > plan_low.0 + 1200 - 1)
    );
}

#[test]
fn t12_5_the_bass_never_plays_two_notes_at_once() {
    for family in FAMILIES {
        for feel in ["groove.rock_8", "groove.bossa", "groove.jazz_ride"] {
            let played = play(&band(family, feel, 8));
            assert!(!played.notes.is_empty(), "{family}");
            for pair in played.notes.windows(2) {
                assert!(pair[0].end() <= pair[1].start, "{family} {feel}");
            }
            assert!(played.notes.iter().all(|n| n.dur > 0 && n.vel >= 1));
        }
    }
}

#[test]
fn t12_6_families_are_named_and_chosen_by_groove() {
    for (name, family) in FAMILIES.iter().zip([
        Family::RootHold,
        Family::RootEighths,
        Family::KickLock,
        Family::Walking,
        Family::OctaveBounce,
        Family::Syncopated,
        Family::TumbaoAnticipated,
    ]) {
        assert_eq!(Family::parse(name), Some(family));
        assert_eq!(Family::parse(&format!("bass.{name}")), Some(family));
    }
    assert_eq!(Family::parse(" walking "), Some(Family::Walking));
    assert_eq!(Family::parse("slap"), None);
    assert_eq!(Family::parse(""), None);
    for (groove, family) in [
        ("groove.house", Family::OctaveBounce),
        ("groove.techno", Family::OctaveBounce),
        ("groove.trap", Family::RootHold),
        ("groove.lofi_swing", Family::RootHold),
        ("groove.boom_bap", Family::KickLock),
        ("groove.jazz_ride", Family::Walking),
        ("groove.funk_16", Family::Syncopated),
        ("groove.one_drop", Family::Syncopated),
        ("groove.afrobeats", Family::Syncopated),
        ("groove.bossa", Family::TumbaoAnticipated),
        ("groove.rock_8", Family::RootEighths),
        ("groove.pop_four", Family::RootEighths),
        ("groove.straight_8", Family::RootEighths),
    ] {
        assert_eq!(Family::for_groove(groove), family, "{groove}");
    }
    // An unknown name in the brief falls back to the groove's own.
    let unknown = play(&band("slap", "groove.jazz_ride", 4));
    let walking = play(&band("walking", "groove.jazz_ride", 4));
    assert_eq!(unknown.notes, walking.notes);
}

#[test]
fn t12_7_drone_sections_hold_the_tonic_and_harmonyless_ones_are_silent() {
    let mut brief = band("walking", "groove.rock_8", 8);
    brief.form[0].harmony = SectionHarmony::Drone {
        degrees: vec![1, 5],
    };
    let played = play(&brief);
    assert!(played.spans.is_empty());
    assert_eq!(played.notes.len(), 1);
    let held = &played.notes[0];
    assert_eq!((held.start, held.end()), (0, 8 * 3840));
    assert_eq!(class(held.pitch.cents), 0);

    brief.form[0].harmony = SectionHarmony::None;
    assert!(play(&brief).notes.is_empty());
    brief.form[0].harmony = progression(&["I"], 2);
    assert!(!play(&brief).notes.is_empty());
}

#[test]
fn t12_8_the_same_seed_gives_the_same_bass_and_another_seed_another_line() {
    let brief = band("syncopated", "groove.funk_16", 16);
    let first = play_with(&brief, 5, None);
    assert_eq!(first.notes, play_with(&brief, 5, None).notes);
    assert!((6..12).any(|seed| play_with(&brief, seed, None).notes != first.notes));
}

#[test]
fn t12_9_quiet_basses_play_fewer_notes_and_busy_ones_more() {
    let count = |busy: f64, pattern: &str| {
        let mut brief = band(pattern, "groove.rock_8", 8);
        brief.tracks[1].busy = busy;
        play(&brief).notes.len()
    };
    assert!(count(0.1, "root_eighths") < count(0.9, "root_eighths"));
    assert!(count(0.1, "syncopated") < count(0.9, "syncopated"));
}
