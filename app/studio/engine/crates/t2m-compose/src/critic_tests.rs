use t2m_core::brief::{Brief, Role};
use t2m_core::{Note, Pitch, Score};

use super::*;
use crate::compose;
use crate::harmony::chord_timeline;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, starter};

fn made(seed: u64) -> (Brief, Score) {
    let brief = starter();
    let score = compose(&brief, kb(), seed);
    (brief, score)
}

fn value(score: &Score, brief: &Brief, id: &str) -> Metric {
    critique(score, brief, kb()).get(id).unwrap().clone()
}

fn lead(score: &mut Score) -> &mut Vec<Note> {
    &mut score
        .tracks
        .iter_mut()
        .find(|t| t.role == Role::Lead)
        .unwrap()
        .notes
}

#[test]
fn t19_1_a_composed_score_passes_every_gate() {
    for seed in 1..=10 {
        let (brief, score) = made(seed);
        let card = critique(&score, &brief, kb());
        assert_eq!(card.metrics.len(), 9);
        assert!(
            card.hard_failures.is_empty(),
            "seed {seed}: {:?}",
            card.hard_failures
        );
        assert!(card.metrics.iter().all(|m| m.passed));
        assert_eq!(card.get("nope"), None);
    }
}

#[test]
fn t19_2_harmonic_agreement_falls_when_strong_beats_leave_the_chord() {
    let (brief, mut score) = made(1);
    assert!(value(&score, &brief, "harmonic_agreement").value >= 0.85);
    for note in lead(&mut score) {
        note.pitch = Pitch { cents: 100 };
    }
    let m = value(&score, &brief, "harmonic_agreement");
    assert_eq!(m.value, 0.0);
    assert!(!m.passed);
    // Without harmony the measure does not apply.
    let mut drone = brief.clone();
    for section in &mut drone.form {
        section.harmony = t2m_core::brief::SectionHarmony::None;
    }
    assert_eq!(value(&score, &drone, "harmonic_agreement").value, 1.0);
}

#[test]
fn t19_3_out_of_system_counts_notes_off_the_scale_and_chord() {
    let (brief, mut score) = made(2);
    assert_eq!(value(&score, &brief, "out_of_system").value, 0.0);
    let bass = score
        .tracks
        .iter_mut()
        .find(|t| t.role == Role::Bass)
        .unwrap();
    bass.notes[0].pitch.cents += 50;
    bass.notes[1].pitch.cents += 50;
    let m = value(&score, &brief, "out_of_system");
    assert_eq!(m.value, 2.0);
    assert!(!m.passed);
    // Drums and effects are not pitched, so they are never off the scale.
    let drums = score
        .tracks
        .iter_mut()
        .find(|t| t.role == Role::Drums)
        .unwrap();
    drums.notes[0].pitch.cents += 50;
    assert_eq!(value(&score, &brief, "out_of_system").value, 2.0);
}

#[test]
fn t19_4_cycle_integrity_wants_whole_bars() {
    let (brief, mut score) = made(1);
    assert_eq!(value(&score, &brief, "cycle_integrity").value, 1.0);
    score.sections[0].end += 100;
    let m = value(&score, &brief, "cycle_integrity");
    assert_eq!(m.value, 0.75);
    assert!(!m.passed);
    score.sections.clear();
    assert_eq!(value(&score, &brief, "cycle_integrity").value, 1.0);
}

#[test]
fn t19_5_register_overlap_counts_a_lead_doubled_at_the_unison() {
    let (brief, mut score) = made(1);
    assert!(value(&score, &brief, "register_overlap").value <= 0.15);
    let copy: Vec<Note> = lead(&mut score).clone();
    score
        .tracks
        .iter_mut()
        .find(|t| t.role == Role::Chords)
        .unwrap()
        .notes = copy;
    let m = value(&score, &brief, "register_overlap");
    assert_eq!(m.value, 1.0);
    assert!(!m.passed);
    lead(&mut score).clear();
    assert_eq!(value(&score, &brief, "register_overlap").value, 0.0);
}

#[test]
fn t19_6_form_contrast_needs_the_loudness_to_follow_the_energy() {
    let (brief, mut score) = made(1);
    assert!(value(&score, &brief, "form_contrast").passed);
    for track in &mut score.tracks {
        for note in &mut track.notes {
            note.vel = 80;
        }
    }
    let m = value(&score, &brief, "form_contrast");
    assert_eq!(m.value, 0.0);
    assert!(!m.passed);
    // A form whose sections share one energy needs no contrast.
    let mut flat = brief.clone();
    for section in &mut flat.form {
        section.energy = 0.5;
    }
    assert!(value(&score, &flat, "form_contrast").passed);
}

#[test]
fn t19_7_motif_recurrence_is_reported_but_never_fails() {
    let (brief, mut score) = made(1);
    let m = value(&score, &brief, "motif_recurrence");
    assert!(m.value > 0.0 && m.value <= 1.0);
    assert!(m.passed && m.gate == Gate::Info);
    lead(&mut score).clear();
    assert_eq!(value(&score, &brief, "motif_recurrence").value, 1.0);
}

#[test]
fn t19_8_groove_realism_fails_on_flat_loudness() {
    let (brief, mut score) = made(1);
    assert_eq!(value(&score, &brief, "groove_realism").value, 1.0);
    for note in &mut score.tracks[0].notes {
        note.vel = 90;
    }
    let m = value(&score, &brief, "groove_realism");
    assert!(m.value < 1.0 && !m.passed);
    for track in &mut score.tracks {
        track.notes.clear();
    }
    assert_eq!(value(&score, &brief, "groove_realism").value, 1.0);
}

#[test]
fn t19_9_brief_adherence_wants_the_briefs_tracks_and_sections() {
    let (brief, score) = made(1);
    assert_eq!(value(&score, &brief, "brief_adherence").value, 1.0);
    let mut short = score.clone();
    short.tracks.pop();
    assert!(!value(&short, &brief, "brief_adherence").passed);
    let mut renamed = score.clone();
    renamed.tracks[0].instrument = "other".into();
    assert_eq!(value(&renamed, &brief, "brief_adherence").value, 0.0);
    let mut role = score.clone();
    role.tracks[1].role = Role::Fx;
    assert_eq!(value(&role, &brief, "brief_adherence").value, 0.0);
    let mut id = score.clone();
    id.sections[0].id = "zzz".into();
    assert_eq!(value(&id, &brief, "brief_adherence").value, 0.0);
    let mut fewer = score.clone();
    fewer.sections.pop();
    assert_eq!(value(&fewer, &brief, "brief_adherence").value, 0.0);
}

#[test]
fn t19_10_playability_is_whether_the_score_checks() {
    let (brief, mut score) = made(1);
    assert_eq!(value(&score, &brief, "playability").value, 1.0);
    let notes = lead(&mut score);
    notes[0].dur = 0;
    let m = value(&score, &brief, "playability");
    assert_eq!(m.value, 0.0);
    assert!(!m.passed);
    assert!(
        critique(&score, &brief, kb())
            .hard_failures
            .contains(&"playability".to_string())
    );
}

#[test]
fn t19_11_a_brief_that_cannot_resolve_fails_adherence_instead_of_panicking() {
    let (brief, score) = made(1);
    // The critic repairs the brief first, so even a ruined one is measured.
    let mut ruined = brief.clone();
    ruined.identity.style = "nothing".into();
    ruined.time.tempo_bpm = 9999;
    let card = critique(&score, &ruined, kb());
    assert_eq!(card.metrics.len(), 9);
}

#[test]
fn t19_12_polish_brings_a_broken_score_back_to_the_system_and_the_chords() {
    for seed in 1..=6 {
        let (brief, mut score) = made(seed);
        // A generator that wanders: some notes off the scale, lead strong beats off the chord.
        for track in &mut score.tracks {
            if matches!(track.role, Role::Bass | Role::Chords | Role::Lead) {
                for note in track.notes.iter_mut().step_by(3) {
                    note.pitch.cents += 50;
                }
            }
        }
        for note in lead(&mut score).iter_mut().step_by(2) {
            note.pitch.cents += 100;
        }
        let rerolls = Rerolls::default();
        let plan = resolve(&brief, kb(), seed, &rerolls).unwrap();
        let timeline = chord_timeline(&plan);
        let before = critique(&score, &brief, kb());
        assert!(
            before.hard_failures.contains(&"out_of_system".to_string()),
            "seed {seed}"
        );
        polish(&mut score, &brief, &plan, &timeline);
        let after = critique(&score, &brief, kb());
        assert_eq!(
            after.get("out_of_system").unwrap().value,
            0.0,
            "seed {seed}"
        );
        assert!(
            after.get("harmonic_agreement").unwrap().passed,
            "seed {seed}"
        );
        assert!(after.get("playability").unwrap().passed, "seed {seed}");
        // A good score is left alone.
        let (brief, good) = made(seed);
        let mut same = good.clone();
        let plan = resolve(&brief, kb(), seed, &rerolls).unwrap();
        polish(&mut same, &brief, &plan, &chord_timeline(&plan));
        assert_eq!(same, good);
    }
}

#[test]
fn t19_13_the_helpers_find_the_nearest_tone_and_fit_ranges() {
    assert_eq!(nearest_in(&[0, 400, 700], 520), 400);
    assert_eq!(nearest_in(&[0, 400, 700], 1150), 1200);
    assert_eq!(nearest_in(&[0, 400, 700], -50), 0);
    assert_eq!(nearest_in(&[], 77), 77);
    assert_eq!(fit(6000, (0, 2000)), 1200);
    assert_eq!(fit(-3000, (0, 2000)), 600);
    assert_eq!(fit(500, (0, 100)), 500);
    assert!(near_class(1205, &[0]) && near_class(-20, &[0]) && !near_class(40, &[0]));
    assert!(Gate::AtLeast(1.0).admits(1.0) && !Gate::AtLeast(1.0).admits(0.99));
    assert!(Gate::AtMost(1.0).admits(1.0) && !Gate::AtMost(1.0).admits(1.01));
    assert!(Gate::Exactly(0.0).admits(0.0) && !Gate::Exactly(0.0).admits(0.1));
}

#[test]
fn t19_14_polish_moves_parts_that_double_the_lead_out_of_the_way() {
    for seed in 1..=4 {
        let (brief, mut score) = made(seed);
        let copy: Vec<Note> = lead(&mut score).clone();
        score
            .tracks
            .iter_mut()
            .find(|t| t.role == Role::Chords)
            .unwrap()
            .notes = copy;
        assert!(!value(&score, &brief, "register_overlap").passed);
        let rerolls = Rerolls::default();
        let plan = resolve(&brief, kb(), seed, &rerolls).unwrap();
        polish(&mut score, &brief, &plan, &chord_timeline(&plan));
        let after = critique(&score, &brief, kb());
        assert!(after.get("register_overlap").unwrap().passed, "seed {seed}");
        assert!(after.get("playability").unwrap().passed, "seed {seed}");
        assert_eq!(after.get("out_of_system").unwrap().value, 0.0);
    }
}
