use t2m_core::brief::{Articulation, Brief, Length, Role};

use t2m_core::Tick;

use super::*;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

fn brief(role: Role, articulation: Articulation, energies: &[f64]) -> Brief {
    with(|b| {
        let mut part = track(
            "p",
            role,
            if role == Role::Drums {
                "kit.standard"
            } else {
                "keys.piano"
            },
        );
        part.articulation = articulation;
        b.tracks = vec![part];
        b.form = energies
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut s = section(&format!("s{i}"), 8, &["p"]);
                s.energy = *e;
                s
            })
            .collect();
        b.time.length = Length::Bars(8 * energies.len() as u16);
    })
}

fn n(start: Tick, dur: Tick, cents: i32, vel: u8) -> Note {
    Note {
        start,
        dur,
        pitch: t2m_core::Pitch { cents },
        vel,
        bend: None,
    }
}

fn run(b: &Brief, section: usize, notes: Vec<Note>) -> Vec<Note> {
    let rerolls = Rerolls::default();
    let plan = resolve(b, kb(), 1, &rerolls).unwrap();
    express(&plan, &plan.tracks[0], &plan.sections[section], notes)
}

#[test]
fn t17_1_velocity_always_stays_in_midi_range() {
    for energy in [0.0, 0.5, 1.0] {
        let b = brief(Role::Lead, Articulation::Normal, &[energy]);
        let notes: Vec<Note> = (0..64)
            .map(|i| n(i * 480, 400, 6000, if i % 2 == 0 { 127 } else { 1 }))
            .collect();
        for played in run(&b, 0, notes) {
            assert!((1..=127).contains(&played.vel));
        }
    }
}

#[test]
fn t17_2_a_louder_section_is_played_louder() {
    let b = brief(Role::Lead, Articulation::Normal, &[0.0, 1.0]);
    let notes =
        |at: Tick| -> Vec<Note> { (0..32).map(|i| n(at + i * 960, 900, 6000, 80)).collect() };
    let mean = |played: Vec<Note>| {
        played.iter().map(|x| f64::from(x.vel)).sum::<f64>() / played.len() as f64
    };
    let quiet = mean(run(&b, 0, notes(0)));
    let loud = mean(run(&b, 1, notes(8 * 3840)));
    assert!(loud - quiet >= 12.0, "{quiet} vs {loud}");
    // Drums have no phrase swell, so only the section scale shows.
    let d = brief(Role::Drums, Articulation::Normal, &[0.0, 1.0]);
    let hit = |at| run(&d, if at == 0 { 0 } else { 1 }, vec![n(at, 100, 3600, 100)])[0].vel;
    assert_eq!(hit(0), 55);
    assert_eq!(hit(8 * 3840), 100);
}

#[test]
fn t17_3_articulation_sets_how_much_of_a_note_sounds() {
    for (articulation, share) in [
        (Articulation::Legato, 960),
        (Articulation::Normal, 864),
        (Articulation::Staccato, 480),
    ] {
        let b = brief(Role::Lead, articulation, &[0.5]);
        let played = run(&b, 0, vec![n(1920, 960, 6000, 80)]);
        assert_eq!(played[0].end(), 1920 + share, "{articulation:?}");
    }
}

#[test]
fn t17_4_drums_keep_their_timing_and_length() {
    let b = brief(Role::Drums, Articulation::Staccato, &[0.5]);
    let notes: Vec<Note> = (0..16)
        .map(|i| n(i * 240, 120, 3600 + (i as i32 % 3) * 100, 90))
        .collect();
    let played = run(&b, 0, notes.clone());
    assert_eq!(played.len(), notes.len());
    for (a, b) in played.iter().zip(&notes) {
        assert_eq!((a.start, a.dur, a.pitch), (b.start, b.dur, b.pitch));
    }
}

#[test]
fn t17_5_timing_moves_only_a_little_and_never_out_of_the_section_or_over_a_note() {
    let b = brief(Role::Lead, Articulation::Legato, &[0.5, 0.5]);
    let rerolls = Rerolls::default();
    let plan = resolve(&b, kb(), 1, &rerolls).unwrap();
    let reach = Tick::from(plan.groove.human_ticks);
    let start = plan.sections[1].start;
    let notes: Vec<Note> = (0..64).map(|i| n(start + i * 480, 480, 6000, 80)).collect();
    let played = express(&plan, &plan.tracks[0], &plan.sections[1], notes.clone());
    let mut moved = 0;
    for (p, w) in played.iter().zip(&notes) {
        assert!((p.start - w.start).abs() <= reach);
        assert!(p.start >= start && p.end() <= w.end());
        moved += i32::from(p.start != w.start);
    }
    assert!(moved > 20, "{moved} of 64 notes moved");
    for pair in played.windows(2) {
        assert!(pair[0].end() <= pair[1].start);
    }
    // The first note of a section never leaves its section, however it is shifted.
    let first = express(
        &plan,
        &plan.tracks[0],
        &plan.sections[1],
        vec![n(start, 100, 6000, 80)],
    );
    assert!(first[0].start >= start);
    // Another section's notes are not touched by this one.
    let again = express(&plan, &plan.tracks[0], &plan.sections[1], notes);
    assert_eq!(again, played);
}

#[test]
fn t17_6_a_phrase_swells_toward_its_middle() {
    let b = brief(Role::Lead, Articulation::Normal, &[1.0]);
    let at = |bar: Tick| run(&b, 0, vec![n(bar * 3840 + 1000, 500, 6000, 80)])[0].vel;
    assert!(at(2) > at(0) + 10);
    assert!(at(2) > at(3));
    assert!(at(4) < at(2));
}
