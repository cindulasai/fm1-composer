use t2m_core::brief::{Brief, Length, Register, Role, SectionHarmony};

use super::*;
use crate::harmony::chord_timeline;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

/// One eight-bar section of chords over `I V vi IV`, a chord a bar.
fn keys(pattern: &str, role: Role) -> Brief {
    with(|b| {
        let mut part = track("keys", role, "keys.piano");
        part.pattern = pattern.into();
        b.tracks = vec![part];
        b.form = vec![section("a", 8, &["keys"])];
        b.time.length = Length::Bars(8);
    })
}

struct Played {
    notes: Vec<Note>,
    spans: Vec<ChordSpan>,
    low: i32,
    high: i32,
    tempo: i64,
}

fn play(brief: &Brief) -> Played {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), 1, &rerolls).expect("a valid brief resolves");
    let spans = chord_timeline(&plan).remove(0);
    let part = plan.track("keys").unwrap();
    Played {
        notes: chords(&plan, part, &plan.sections[0], &spans),
        spans,
        low: part.low,
        high: part.high,
        tempo: i64::from(plan.tempo),
    }
}

fn in_span<'a>(played: &'a Played, span: &ChordSpan) -> Vec<&'a Note> {
    played
        .notes
        .iter()
        .filter(|n| n.start >= span.start && n.start < span.end)
        .collect()
}

#[test]
fn t12_10_sustained_chords_are_exactly_the_voicing_and_stop_with_the_chord() {
    let played = play(&keys("sustain", Role::Chords));
    assert_eq!(played.spans.len(), 8);
    for span in &played.spans {
        let mut got: Vec<i32> = in_span(&played, span)
            .iter()
            .map(|n| n.pitch.cents)
            .collect();
        got.sort_unstable();
        let want: Vec<i32> = span.voicing.iter().map(|p| p.cents).collect();
        assert_eq!(got, want);
        for note in in_span(&played, span) {
            assert_eq!((note.start, note.end()), (span.start, span.end));
        }
    }
}

#[test]
fn t12_11_a_strum_sounds_low_to_high_a_few_milliseconds_apart() {
    let played = play(&keys("strum", Role::Chords));
    for span in &played.spans {
        let mut chord = in_span(&played, span);
        chord.sort_by_key(|n| n.pitch.cents);
        assert!(chord.len() >= 3);
        for pair in chord.windows(2) {
            assert!(pair[0].start < pair[1].start, "onsets rise with pitch");
            let ms = (pair[1].start - pair[0].start) * 60_000 / (played.tempo * 960);
            assert!((9..=31).contains(&ms), "{ms} ms between strings");
        }
        assert!(chord.iter().all(|n| n.end() <= span.end));
    }
}

#[test]
fn t12_12_denser_sections_play_more_hits() {
    for pattern in ["comping", "stabs"] {
        let count = |density: f64| {
            let mut brief = keys(pattern, Role::Chords);
            brief.form[0].density = density;
            play(&brief).notes.len()
        };
        assert!(count(0.9) > count(0.2), "{pattern}");
    }
}

#[test]
fn t12_13_a_drone_section_holds_the_named_degrees_all_the_way_through() {
    for role in [Role::Chords, Role::Pad, Role::Drone] {
        let mut brief = keys("", role);
        brief.form[0].harmony = SectionHarmony::Drone {
            degrees: vec![1, 5],
        };
        let played = play(&brief);
        let mut classes: Vec<i32> = played
            .notes
            .iter()
            .map(|n| n.pitch.cents.rem_euclid(1200))
            .collect();
        classes.sort_unstable();
        assert_eq!(classes, [0, 700], "{role:?}");
        assert!(
            played
                .notes
                .iter()
                .all(|n| (n.start, n.end()) == (0, 8 * 3840))
        );
    }
    // A drone part under chords holds the tonic.
    let under = play(&keys("", Role::Drone));
    assert!(under.spans.len() == 8);
    assert!(
        under
            .notes
            .iter()
            .all(|n| n.pitch.cents.rem_euclid(1200) == 0)
    );
    // No harmony, no chords.
    let mut none = keys("sustain", Role::Chords);
    none.form[0].harmony = SectionHarmony::None;
    assert!(play(&none).notes.is_empty());
}

#[test]
fn t12_14_every_family_stays_in_range_and_in_the_section() {
    for pattern in ["sustain", "stabs", "strum", "comping", "broken", ""] {
        for register in [Register::Low, Register::Mid, Register::High] {
            let mut brief = keys(pattern, Role::Chords);
            brief.tracks[0].register = register;
            let played = play(&brief);
            assert!(!played.notes.is_empty(), "{pattern}");
            for note in &played.notes {
                assert!(note.pitch.cents >= played.low && note.pitch.cents <= played.high);
                assert!(note.start >= 0 && note.end() <= 8 * 3840 && note.dur > 0);
            }
        }
    }
}

#[test]
fn t12_15_families_are_named_and_chosen_by_role_and_groove() {
    for (name, family) in [
        ("sustain", Family::Sustain),
        ("stabs", Family::Stabs),
        ("strum", Family::Strum),
        ("comping", Family::Comping),
        ("broken", Family::Broken),
        ("drone", Family::Drone),
    ] {
        assert_eq!(Family::parse(name), Some(family));
        assert_eq!(Family::parse(&format!("chords.{name}")), Some(family));
    }
    assert_eq!(Family::parse("oompah"), None);
    assert_eq!(
        Family::default_for(Role::Drone, "groove.rock_8"),
        Family::Drone
    );
    assert_eq!(
        Family::default_for(Role::Pad, "groove.house"),
        Family::Sustain
    );
    for (groove, family) in [
        ("groove.house", Family::Stabs),
        ("groove.one_drop", Family::Stabs),
        ("groove.funk_16", Family::Comping),
        ("groove.bossa", Family::Comping),
        ("groove.rock_8", Family::Strum),
        ("groove.lofi_swing", Family::Broken),
        ("groove.pop_four", Family::Sustain),
    ] {
        assert_eq!(
            Family::default_for(Role::Chords, groove),
            family,
            "{groove}"
        );
    }
    // `drone` named for a track under real chords is not honoured.
    let named = play(&keys("drone", Role::Chords));
    assert_eq!(named.notes, play(&keys("", Role::Chords)).notes);
}

#[test]
fn t12_16_a_broken_chord_climbs_one_note_at_a_time() {
    let played = play(&keys("broken", Role::Chords));
    let first = &played.spans[0];
    let line: Vec<i32> = in_span(&played, first)
        .iter()
        .take(3)
        .map(|n| n.pitch.cents)
        .collect();
    let want: Vec<i32> = first.voicing.iter().map(|p| p.cents).collect();
    assert_eq!(line, want[..3]);
    for pair in played.notes.windows(2).take(6) {
        assert_eq!(pair[1].start - pair[0].start, 480);
    }
}
