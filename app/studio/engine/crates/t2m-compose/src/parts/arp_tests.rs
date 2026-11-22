use t2m_core::brief::{Brief, Length, Role};

use super::*;
use crate::harmony::chord_timeline;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

fn pluck(pattern: &str, busy: f64) -> Brief {
    with(|b| {
        let mut part = track("arp", Role::Arp, "keys.piano");
        part.pattern = pattern.into();
        part.busy = busy;
        b.tracks = vec![part];
        b.form = vec![section("a", 8, &["arp"])];
        b.time.length = Length::Bars(8);
    })
}

fn play(brief: &Brief, seed: u64) -> (Vec<Note>, Vec<ChordSpan>, (i32, i32)) {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).unwrap();
    let spans = chord_timeline(&plan).remove(0);
    let part = plan.track("arp").unwrap();
    (
        arp(&plan, part, &plan.sections[0], &spans),
        spans,
        (part.low, part.high),
    )
}

fn in_span<'a>(notes: &'a [Note], span: &ChordSpan) -> Vec<&'a Note> {
    notes
        .iter()
        .filter(|n| n.start >= span.start && n.start < span.end)
        .collect()
}

#[test]
fn t15_1_every_note_is_a_tone_of_the_chord_in_range() {
    for pattern in ["up", "down", "updown", "random_seeded", ""] {
        let (notes, spans, (low, high)) = play(&pluck(pattern, 0.7), 2);
        for span in &spans {
            let classes: Vec<i32> = span
                .voicing
                .iter()
                .map(|p| p.cents.rem_euclid(1200))
                .collect();
            let mine = in_span(&notes, span);
            assert!(!mine.is_empty(), "{pattern}");
            for n in mine {
                assert!(
                    classes.contains(&n.pitch.cents.rem_euclid(1200)),
                    "{pattern}"
                );
                assert!(n.pitch.cents >= low && n.pitch.cents <= high);
                assert!(n.end() <= span.end);
            }
        }
    }
}

#[test]
fn t15_2_the_rate_follows_busy() {
    let (slow, spans, _) = play(&pluck("up", 0.49), 1);
    let (fast, _, _) = play(&pluck("up", 0.5), 1);
    assert_eq!(in_span(&slow, &spans[0]).len(), 8);
    assert_eq!(in_span(&fast, &spans[0]).len(), 16);
    assert_eq!(slow[1].start - slow[0].start, 480);
    assert_eq!(fast[1].start - fast[0].start, 240);
}

#[test]
fn t15_3_each_shape_moves_the_way_it_is_named() {
    let first = |pattern| {
        let (notes, spans, _) = play(&pluck(pattern, 0.9), 1);
        in_span(&notes, &spans[0])
            .iter()
            .map(|n| n.pitch.cents)
            .collect::<Vec<i32>>()
    };
    let up = first("up");
    assert!(up.windows(2).take(3).all(|p| p[0] < p[1]));
    let down = first("down");
    assert!(down.windows(2).take(3).all(|p| p[0] > p[1]));
    let updown = first("updown");
    let top = updown
        .iter()
        .position(|p| p == updown.iter().max().unwrap())
        .unwrap();
    assert!(top > 0 && top < updown.len() - 1);
    assert!(updown[..=top].windows(2).all(|p| p[0] < p[1]));
    assert!(updown[top..].windows(2).take(2).all(|p| p[0] > p[1]));
    let random = first("random_seeded");
    assert!(random.windows(2).any(|p| p[0] < p[1]) && random.windows(2).any(|p| p[0] > p[1]));
    assert_eq!(first("arp.up"), up);
    assert_eq!(first("nonsense"), up);
}

#[test]
fn t15_4_shapes_have_names_and_the_random_one_follows_the_seed() {
    for (name, shape) in [
        ("up", Shape::Up),
        ("down", Shape::Down),
        ("updown", Shape::UpDown),
        ("random_seeded", Shape::Random),
        ("random", Shape::Random),
    ] {
        assert_eq!(Shape::parse(name), Some(shape));
        assert_eq!(Shape::parse(&format!("arp.{name}")), Some(shape));
    }
    assert_eq!(Shape::parse(" up "), Some(Shape::Up));
    assert_eq!(Shape::parse(""), None);
    let brief = pluck("random_seeded", 0.5);
    assert_eq!(play(&brief, 4).0, play(&brief, 4).0);
    assert!((5..12).any(|s| play(&brief, s).0 != play(&brief, 4).0));
}

#[test]
fn t15_5_beats_are_stressed_and_a_chordless_section_is_silent() {
    let (notes, _, _) = play(&pluck("up", 0.2), 1);
    assert_eq!((notes[0].vel, notes[1].vel, notes[2].vel), (90, 76, 90));
    let mut none = pluck("up", 0.2);
    none.form[0].harmony = t2m_core::brief::SectionHarmony::None;
    assert!(play(&none, 1).0.is_empty());
}
