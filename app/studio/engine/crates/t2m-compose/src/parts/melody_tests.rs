use t2m_core::brief::{Brief, Length, MeterSpec, Role, SectionHarmony};

use super::*;
use crate::harmony::{chord_at, chord_timeline};
use crate::motif::motif;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, progression, section, track, with};

/// One section of lead over `I V vi IV`, a chord a bar.
fn song(bars: u16, busy: f64) -> Brief {
    with(|b| {
        let mut part = track("lead", Role::Lead, "keys.piano");
        part.busy = busy;
        b.tracks = vec![part];
        b.form = vec![section("a", bars, &["lead"])];
        b.time.length = Length::Bars(bars);
    })
}

struct Played {
    notes: Vec<Note>,
    spans: Vec<ChordSpan>,
    cell: Cell,
    strong: Vec<Tick>,
    low: i32,
    high: i32,
    bar: Tick,
    end: Tick,
}

fn play(brief: &Brief, seed: u64) -> Played {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).expect("a valid brief resolves");
    let spans = chord_timeline(&plan).remove(0);
    let part = plan.track("lead").unwrap();
    let cell = motif(&plan, "lead");
    let section = &plan.sections[0];
    Played {
        notes: lead(&plan, part, section, &spans, &cell),
        spans,
        cell,
        strong: plan.frame.strong_positions(),
        low: part.low,
        high: part.high,
        bar: plan.span,
        end: section.end,
    }
}

fn classes_at(played: &Played, at: Tick) -> Vec<i32> {
    chord_at(&played.spans, at)
        .map(|span| {
            span.chord
                .tones
                .iter()
                .map(|t| t.cents.rem_euclid(1200))
                .collect()
        })
        .unwrap_or_default()
}

fn is_chord_tone(played: &Played, note: &Note) -> bool {
    classes_at(played, note.start).contains(&note.pitch.cents.rem_euclid(1200))
}

fn steps(a: i32, b: i32) -> i32 {
    let system = kb().tonal_system("mode.major").unwrap();
    let tuning = kb().tuning("12tet").unwrap();
    let degree = |cents| system.nearest_degree(tuning, t2m_core::Pitch { cents });
    (degree(a) - degree(b)).abs()
}

const BUSY: [f64; 3] = [0.2, 0.5, 0.8];

#[test]
fn t13_5_strong_beats_carry_chord_tones() {
    for busy in BUSY {
        for seed in 1..=8 {
            let played = play(&song(8, busy), seed);
            let strong: Vec<&Note> = played
                .notes
                .iter()
                .filter(|n| played.strong.contains(&(n.start % played.bar)))
                .collect();
            assert!(strong.len() >= 8, "{} notes on strong beats", strong.len());
            let good = strong.iter().filter(|n| is_chord_tone(&played, n)).count();
            assert!(
                good * 100 >= strong.len() * 85,
                "busy {busy} seed {seed}: {good}/{}",
                strong.len()
            );
        }
    }
}

#[test]
fn t13_6_a_note_outside_the_chord_is_reached_or_left_by_step() {
    for busy in BUSY {
        for seed in 1..=8 {
            let played = play(&song(8, busy), seed);
            for (index, n) in played.notes.iter().enumerate() {
                if is_chord_tone(&played, n) {
                    continue;
                }
                let before = index
                    .checked_sub(1)
                    .map(|i| steps(played.notes[i].pitch.cents, n.pitch.cents));
                let after = played
                    .notes
                    .get(index + 1)
                    .map(|m| steps(m.pitch.cents, n.pitch.cents));
                let by_step = [before, after].into_iter().flatten().any(|s| s <= 2);
                assert!(
                    by_step,
                    "busy {busy} seed {seed} note {index}: {:?}",
                    played.notes[index.saturating_sub(2)..(index + 3).min(played.notes.len())]
                        .iter()
                        .map(|n| (n.start, n.dur, n.pitch.cents))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn t13_7_every_phrase_ends_on_a_held_note_and_a_rest() {
    for bars in [4u16, 8, 16, 6] {
        for seed in 1..=4 {
            let played = play(&song(bars, 0.6), seed);
            let phrase = if bars.is_multiple_of(4) { 4 } else { 2 } * played.bar;
            let mut start = 0;
            while start < played.end {
                let end = (start + phrase).min(played.end);
                let last = played
                    .notes
                    .iter()
                    .rfind(|n| n.start >= start && n.start < end)
                    .unwrap();
                assert!(last.dur >= 960, "bars {bars}: held {}", last.dur);
                assert!(
                    last.end() + 480 <= end,
                    "bars {bars}: rest {}",
                    end - last.end()
                );
                start = end;
            }
        }
    }
}

#[test]
fn t13_8_phrases_open_with_the_motif_and_do_not_repeat_themselves_over_and_over() {
    for seed in 1..=6 {
        let played = play(&song(16, 0.5), seed);
        let phrase = 4 * played.bar;
        let mut seen: Vec<Vec<(Tick, Tick, i32)>> = Vec::new();
        let mut opening = 0;
        for index in 0..4 {
            let start = index * phrase;
            let notes: Vec<&Note> = played
                .notes
                .iter()
                .filter(|n| n.start >= start && n.start < start + phrase)
                .collect();
            // The phrase begins with the motif's rhythm (the first note is always whole).
            let mut at = 0;
            let mut matches = true;
            for (note, dur) in notes.iter().zip(&played.cell.durs) {
                matches &= note.start - start == at && note.dur <= *dur;
                at += dur;
            }
            opening += usize::from(matches);
            seen.push(
                notes
                    .iter()
                    .map(|n| (n.start - start, n.dur, n.pitch.cents))
                    .collect(),
            );
        }
        assert!(
            opening >= 2,
            "seed {seed}: {opening} of 4 phrases open with the motif"
        );
        for phrase in &seen {
            assert!(seen.iter().filter(|other| *other == phrase).count() <= 2);
        }
    }
}

#[test]
fn t13_9_one_highest_note_in_the_second_half() {
    for busy in BUSY {
        for bars in [1u16, 2, 8, 16] {
            for seed in 1..=8 {
                let played = play(&song(bars, busy), seed);
                let top = played.notes.iter().map(|n| n.pitch.cents).max().unwrap();
                let at: Vec<Tick> = played
                    .notes
                    .iter()
                    .filter(|n| n.pitch.cents == top)
                    .map(|n| n.start)
                    .collect();
                assert_eq!(at.len(), 1, "bars {bars} busy {busy} seed {seed}: {at:?}");
                assert!(
                    at[0] >= played.end / 2,
                    "bars {bars} seed {seed}: peak at {} of {}",
                    at[0],
                    played.end
                );
            }
        }
    }
}

#[test]
fn t13_10_the_line_is_single_in_register_and_in_range() {
    for busy in BUSY {
        for seed in 1..=8 {
            let played = play(&song(8, busy), seed);
            for pair in played.notes.windows(2) {
                assert!(pair[0].end() <= pair[1].start);
            }
            for n in &played.notes {
                assert!(n.pitch.cents >= played.low && n.pitch.cents <= played.high);
                assert!(n.start >= 0 && n.end() <= played.end && n.dur > 0);
            }
        }
    }
}

#[test]
fn t13_11_a_busier_part_plays_more_notes() {
    let per_bar = |busy: f64| {
        (1..=6)
            .map(|seed| play(&song(8, busy), seed).notes.len())
            .sum::<usize>()
    };
    assert!(per_bar(0.8) > per_bar(0.2) + 40);
}

#[test]
fn t13_12_the_same_seed_gives_the_same_melody() {
    let brief = song(8, 0.5);
    assert_eq!(play(&brief, 3).notes, play(&brief, 3).notes);
    assert!((4..12).any(|seed| play(&brief, seed).notes != play(&brief, 3).notes));
}

#[test]
fn t13_13_a_section_without_chords_sings_over_the_tonic_triad() {
    for harmony in [
        SectionHarmony::Drone {
            degrees: vec![1, 5],
        },
        SectionHarmony::None,
    ] {
        let mut brief = song(8, 0.5);
        brief.form[0].harmony = harmony;
        let played = play(&brief, 2);
        assert!(played.spans.is_empty());
        assert!(played.notes.len() > 8);
        let triad = [0, 400, 700];
        let strong: Vec<&Note> = played
            .notes
            .iter()
            .filter(|n| played.strong.contains(&(n.start % played.bar)))
            .collect();
        assert!(
            strong
                .iter()
                .all(|n| triad.contains(&n.pitch.cents.rem_euclid(1200)))
        );
    }
}

#[test]
fn t13_14_odd_meters_and_tiny_sections_still_give_a_valid_line() {
    let meters = [
        (7, 8, Some(vec![2, 2, 3])),
        (6, 8, None),
        (3, 4, None),
        (5, 4, None),
        (2, 4, None),
    ];
    for (numerator, denominator, grouping) in meters {
        for bars in [1u16, 3, 5] {
            let mut brief = song(bars, 0.7);
            brief.time.meter = MeterSpec::Meter {
                numerator,
                denominator,
                grouping: grouping.clone(),
            };
            let played = play(&brief, 1);
            assert!(
                !played.notes.is_empty(),
                "{numerator}/{denominator} x {bars}"
            );
            for pair in played.notes.windows(2) {
                assert!(pair[0].end() <= pair[1].start);
            }
            assert!(
                played
                    .notes
                    .iter()
                    .all(|n| n.start >= 0 && n.end() <= played.end && n.dur > 0)
            );
        }
    }
    brief_with_progression();
}

fn brief_with_progression() {
    // A chord every two bars still leaves strong beats on chord tones.
    let mut brief = song(8, 0.5);
    brief.form[0].harmony = progression(&["I", "IV"], 2);
    let played = play(&brief, 1);
    let strong: Vec<&Note> = played
        .notes
        .iter()
        .filter(|n| played.strong.contains(&(n.start % played.bar)))
        .collect();
    let good = strong.iter().filter(|n| is_chord_tone(&played, n)).count();
    assert!(good * 100 >= strong.len() * 85);
}
