use t2m_core::brief::{Brief, Length, Role};

use super::*;
use crate::harmony::chord_timeline;
use crate::motif::motif;
use crate::parts::melody::lead;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

fn duet(busy: f64) -> Brief {
    with(|b| {
        let mut second = track("counter", Role::Counter, "keys.piano");
        second.busy = busy;
        let mut first = track("lead", Role::Lead, "keys.piano");
        first.busy = 0.4;
        b.tracks = vec![first, second];
        b.form = vec![section("a", 8, &["lead", "counter"])];
        b.time.length = Length::Bars(8);
    })
}

fn play(brief: &Brief, seed: u64) -> (Vec<Note>, Vec<Note>) {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).unwrap();
    let spans = chord_timeline(&plan).remove(0);
    let section = &plan.sections[0];
    let cell = motif(&plan, "lead");
    let melody = lead(&plan, plan.track("lead").unwrap(), section, &spans, &cell);
    let line = counter(
        &plan,
        plan.track("counter").unwrap(),
        section,
        &spans,
        &melody,
    );
    (melody, line)
}

fn sounding(lead: &[Note], at: Tick) -> Option<&Note> {
    lead.iter().find(|n| n.start <= at && at < n.end())
}

#[test]
fn t14_1_the_counter_speaks_where_the_lead_rests_or_holds() {
    for seed in 1..=8 {
        let (melody, line) = play(&duet(0.7), seed);
        assert!(line.len() >= 4, "seed {seed}: {} notes", line.len());
        let open = line
            .iter()
            .filter(|n| {
                !melody.iter().any(|m| m.start == n.start)
                    && sounding(&melody, n.start).is_none_or(|m| m.dur >= 960)
            })
            .count();
        assert!(
            open * 10 >= line.len() * 6,
            "seed {seed}: {open} of {}",
            line.len()
        );
    }
}

#[test]
fn t14_2_the_counter_stays_below_the_lead_and_off_its_pitch_class() {
    for seed in 1..=8 {
        let (melody, line) = play(&duet(0.8), seed);
        let mut doubled = 0;
        let mut together = 0;
        for n in &line {
            if let Some(m) = sounding(&melody, n.start) {
                together += 1;
                assert!(n.pitch.cents < m.pitch.cents, "seed {seed}");
                doubled +=
                    usize::from(n.pitch.cents.rem_euclid(1200) == m.pitch.cents.rem_euclid(1200));
            }
        }
        assert!(doubled * 5 <= together.max(1), "{doubled} of {together}");
    }
}

#[test]
fn t14_3_the_line_is_single_in_range_and_in_the_section() {
    for seed in 1..=6 {
        let (_, line) = play(&duet(0.9), seed);
        for pair in line.windows(2) {
            assert!(pair[0].end() <= pair[1].start);
        }
        assert!(
            line.iter()
                .all(|n| n.start >= 0 && n.end() <= 8 * 3840 && n.dur > 0 && n.dur <= 1920)
        );
    }
}

#[test]
fn t14_4_a_busier_counter_plays_more_and_the_seed_decides_the_notes() {
    let count = |busy: f64| (1..=6).map(|s| play(&duet(busy), s).1.len()).sum::<usize>();
    assert!(count(0.9) > count(0.1));
    let brief = duet(0.6);
    assert_eq!(play(&brief, 3).1, play(&brief, 3).1);
    assert!((4..12).any(|s| play(&brief, s).1 != play(&brief, 3).1));
}

#[test]
fn t14_5_open_slots_are_rests_or_long_holds_only() {
    let held = |start, dur| Note {
        start,
        dur,
        pitch: t2m_core::Pitch { cents: 6000 },
        vel: 90,
        bend: None,
    };
    let lead = [held(0, 960), held(960, 959), held(1919, 100)];
    assert_eq!(open_slot(&lead, 0, 960), None);
    assert_eq!(open_slot(&lead, 480, 960), Some(Some(6000)));
    assert_eq!(open_slot(&lead, 960, 960), None);
    assert_eq!(open_slot(&lead, 1200, 960), None);
    assert_eq!(open_slot(&lead, 1900, 960), None);
    assert_eq!(open_slot(&lead, 2100, 960), Some(None));
    assert_eq!(open_slot(&lead, 5000, 960), Some(None));
    assert_eq!(open_slot(&[], 0, 960), Some(None));
}
