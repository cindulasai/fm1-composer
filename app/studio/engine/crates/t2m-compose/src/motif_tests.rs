use t2m_core::brief::{Length, Motif};

use super::*;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, with};

fn cell(degrees: &[i32], durs: &[Tick]) -> Cell {
    Cell {
        degrees: degrees.to_vec(),
        durs: durs.to_vec(),
        offset: 0,
    }
}

fn major() -> TonalSystem {
    kb().tonal_system("mode.major").unwrap().clone()
}

fn motif_of(brief: &t2m_core::brief::Brief, seed: u64) -> Cell {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).unwrap();
    motif(&plan, "lead")
}

fn given(degrees: &[i32], durations: &[f64]) -> t2m_core::brief::Brief {
    with(|b| {
        b.motif = Some(Motif {
            degrees: degrees.to_vec(),
            durations: durations.to_vec(),
            development: String::new(),
        });
        b.time.length = Length::Bars(24);
    })
}

#[test]
fn t13_1_steps_a_system_forbids_are_snapped_and_the_length_is_kept() {
    let mut raga = major();
    raga.moves.ascending = vec![0, 1, 2, 4, 5];
    let snapped = snap(&cell(&[0, 3, 4, 6], &[480, 480, 960, 480]), &raga);
    assert_eq!(snapped.degrees.len(), 4);
    assert_eq!(snapped.durs, [480, 480, 960, 480]);
    for pair in snapped.degrees.windows(2) {
        assert!(raga.legal_move(pair[0], pair[1]), "{pair:?}");
    }
    assert_eq!(snapped.degrees[..2], [0, 2]);
    // A legal cell is left as it is; so is a single note.
    let fine = cell(&[0, 1, 2, 4], &[1; 4]);
    assert_eq!(snap(&fine, &raga), fine);
    assert_eq!(snap(&cell(&[3], &[10]), &raga).degrees, [3]);
    // Nothing legal anywhere: the note repeats.
    let mut stuck = major();
    stuck.moves.ascending = vec![0];
    stuck.moves.descending = vec![0];
    assert_eq!(snap(&cell(&[0, 3], &[1, 1]), &stuck).degrees, [0, 0]);
}

#[test]
fn t13_2_each_development_changes_what_it_says() {
    let base = cell(&[0, 2, 1], &[480, 480, 960]);
    let system = major();
    let go = |op| develop(&base, op, &system);
    assert_eq!(go(DevOp::Repeat), base);
    assert_eq!(go(DevOp::Sequence(1)).degrees, [1, 3, 2]);
    assert_eq!(go(DevOp::Sequence(-2)).degrees, [-2, 0, -1]);
    assert_eq!(go(DevOp::Invert).degrees, [0, -2, -1]);
    assert_eq!(go(DevOp::Augment).durs, [960, 960, 1920]);
    assert_eq!(go(DevOp::Diminish).durs, [240, 240, 480]);
    assert_eq!(
        cell(&[0], &[1]).durs,
        develop(&cell(&[0], &[1]), DevOp::Diminish, &system).durs
    );
    assert_eq!(go(DevOp::Fragment(2)).degrees, [0, 2]);
    assert_eq!(go(DevOp::Fragment(2)).durs, [480, 480]);
    assert_eq!(go(DevOp::Fragment(0)).degrees.len(), 1);
    assert_eq!(go(DevOp::Fragment(9)).degrees.len(), 3);
    assert_eq!(go(DevOp::Displace(240)).offset, 240);
    assert_eq!(go(DevOp::Displace(-500)).offset, 0);
    assert_eq!(go(DevOp::Displace(240)).length(), 240 + 1920);
    // The answer ends on the tonic's pitch class, in the octave nearest the note it replaces.
    let high = cell(&[0, 2, 4], &[1, 1, 1]);
    let answer = develop(&high, DevOp::AnswerTo(0), &system);
    assert_eq!(answer.degrees, [0, 2, 7]);
    assert_eq!(
        develop(&high, DevOp::AnswerTo(2), &system).degrees,
        [0, 2, 2]
    );
    assert_eq!(
        develop(&high, DevOp::AnswerTo(-1), &system).degrees,
        [0, 2, 6]
    );
    // The original is never touched.
    assert_eq!(base.degrees, [0, 2, 1]);
}

#[test]
fn t13_3_the_motif_is_the_briefs_or_an_invented_one_that_never_changes() {
    let made = given(&[0, 2, 4], &[0.5, 0.5, 1.0]);
    let got = motif_of(&made, 1);
    assert_eq!(got.degrees, [0, 2, 4]);
    assert_eq!(got.durs, [480, 480, 960]);
    // More degrees than durations: the shorter list decides.
    assert_eq!(motif_of(&given(&[0, 2, 4], &[1.0]), 1).degrees, [0]);
    // A length that is not a number is a beat; an empty one is replaced.
    assert_eq!(
        motif_of(&given(&[0, 1], &[f64::NAN, f64::INFINITY]), 1).durs,
        [960, 960]
    );
    // Repair caps a length at sixteen beats before the composer sees it.
    assert_eq!(motif_of(&given(&[0], &[1e12]), 1).durs, [960 * 16]);
    assert!(motif_of(&given(&[0], &[-3.0]), 1).durs[0] > 0);
    assert!(!motif_of(&given(&[], &[]), 1).degrees.is_empty());
    // At most eight notes are kept.
    assert_eq!(motif_of(&given(&[0; 12], &[1.0; 12]), 1).degrees.len(), 8);

    let none = with(|_| {});
    for seed in 1..20 {
        let a = motif_of(&none, seed);
        assert_eq!(a, motif_of(&none, seed));
        assert!((3..=4).contains(&a.degrees.len()), "{a:?}");
        assert_eq!(a.degrees.len(), a.durs.len());
        assert!(a.length() <= 2 * 960 && a.durs.iter().all(|d| *d > 0));
        assert!(a.degrees.iter().all(|d| (-3..=8).contains(d)));
    }
    assert!((1..20).any(|seed| motif_of(&none, seed) != motif_of(&none, 1)));
}

#[test]
fn t13_4_the_invented_motif_is_the_same_in_every_section_and_after_a_reroll() {
    let none = with(|_| {});
    let rerolls = Rerolls::default();
    let plan = resolve(&none, kb(), 7, &rerolls).unwrap();
    let a = motif(&plan, "lead");
    let mut bumped = Rerolls::default();
    bumped.bump("lead", "a");
    bumped.bump("lead", "b");
    let other = resolve(&none, kb(), 7, &bumped).unwrap();
    assert_eq!(motif(&other, "lead"), a);
    assert_ne!(motif(&plan, "bass"), motif(&plan, "lead"));
}
