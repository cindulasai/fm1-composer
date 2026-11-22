use t2m_core::brief::{Brief, Length, Role, Transition};

use super::*;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

/// Three four-bar sections; the first ends with `leaving`.
fn show(leaving: Transition) -> Brief {
    with(|b| {
        b.tracks = vec![track("fx", Role::Fx, "keys.piano")];
        b.form = vec![
            section("a", 4, &["fx"]),
            section("b", 4, &["fx"]),
            section("c", 4, &["fx"]),
        ];
        b.form[0].transition = leaving;
        b.time.length = Length::Bars(12);
    })
}

fn play(brief: &Brief) -> Vec<Vec<Note>> {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), 1, &rerolls).unwrap();
    let part = plan.track("fx").unwrap();
    plan.sections.iter().map(|s| fx(&plan, part, s)).collect()
}

#[test]
fn t16_1_a_riser_fills_the_last_bar_and_climbs() {
    let played = play(&show(Transition::Riser));
    assert_eq!(played[0].len(), 1);
    let rise = &played[0][0];
    assert_eq!((rise.start, rise.dur), (3 * 3840, 3840));
    assert_eq!(rise.bend.as_ref().unwrap().0, [(0, 0), (3840, 1200)]);
    assert!(played[1].is_empty() && played[2].is_empty());
}

#[test]
fn t16_2_an_impact_lands_on_the_first_beat_after() {
    let played = play(&show(Transition::Impact));
    assert!(played[0].is_empty());
    assert_eq!(played[1].len(), 1);
    assert_eq!((played[1][0].start, played[1][0].dur), (4 * 3840, 1920));
    assert!(played[1][0].bend.is_none() && played[2].is_empty());
}

#[test]
fn t16_3_nothing_else_makes_an_effect() {
    for leaving in [
        Transition::None,
        Transition::Crash,
        Transition::FillHeavy,
        Transition::Break,
    ] {
        assert!(
            play(&show(leaving)).iter().all(Vec::is_empty),
            "{leaving:?}"
        );
    }
}

#[test]
fn t16_4_short_sections_stay_inside_themselves() {
    let mut brief = show(Transition::Impact);
    brief.form[1].bars = 1;
    brief.time.length = Length::Bars(9);
    let played = play(&brief);
    assert_eq!(played[1][0].dur, 1920);
    let mut rising = show(Transition::Riser);
    rising.form[0].bars = 1;
    rising.time.length = Length::Bars(9);
    let notes = play(&rising);
    assert_eq!((notes[0][0].start, notes[0][0].dur), (0, 3840));
    for section in notes {
        assert!(section.iter().all(|n| n.dur > 0 && n.end() <= 12 * 3840));
    }
}
