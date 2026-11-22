use super::*;
use crate::plan::PlanSection;
use t2m_core::brief::{SectionHarmony, Transition};

fn span(start: Tick, end: Tick) -> PlanSection {
    PlanSection {
        id: "a".into(),
        start,
        end,
        bars: 1,
        energy: 0.5,
        density: 0.5,
        harmony: SectionHarmony::None,
        active: Default::default(),
        transition: Transition::None,
        entered_by: Transition::None,
    }
}

fn shape(notes: &[Note]) -> Vec<(Tick, Tick, i32)> {
    notes
        .iter()
        .map(|n| (n.start, n.dur, n.pitch.cents))
        .collect()
}

#[test]
fn t11_1_velocity_is_clamped_to_midi_range() {
    assert_eq!(velocity(0), 1);
    assert_eq!(velocity(-50), 1);
    assert_eq!(velocity(1), 1);
    assert_eq!(velocity(64), 64);
    assert_eq!(velocity(127), 127);
    assert_eq!(velocity(128), 127);
    assert_eq!(velocity(i64::MAX), 127);
    let made = note(10, 20, 700, 300);
    assert_eq!(
        (
            made.start,
            made.dur,
            made.pitch.cents,
            made.vel,
            made.bend.clone()
        ),
        (10, 20, 700, 127, None)
    );
}

#[test]
fn t11_2_tidy_keeps_notes_inside_the_section_in_order_without_retriggering() {
    let section = span(1000, 2000);
    // Before the section, past its end, and out of order.
    let notes = vec![
        note(1900, 500, 0, 90),
        note(900, 50, 700, 90),
        note(1500, 100, 400, 90),
        note(5000, 10, 100, 90),
    ];
    let tidied = tidy(notes, &section);
    assert_eq!(
        shape(&tidied),
        [
            (1000, 50, 700),
            (1500, 100, 400),
            (1900, 100, 0),
            (1999, 1, 100)
        ]
    );
    // A note of the same pitch arriving while one sounds cuts the earlier one short.
    let notes = vec![
        note(1000, 600, 0, 90),
        note(1400, 100, 0, 90),
        note(1200, 100, 700, 90),
    ];
    assert_eq!(
        shape(&tidy(notes, &section)),
        [(1000, 400, 0), (1200, 100, 700), (1400, 100, 0)]
    );
    // Two notes of one pitch at one instant: the first has no time to sound.
    let notes = vec![note(1000, 100, 0, 90), note(1000, 100, 0, 80)];
    let tidied = tidy(notes, &section);
    assert_eq!(shape(&tidied), [(1000, 100, 0)]);
    // Notes that only touch are left alone; other pitches may overlap freely.
    let notes = vec![
        note(1000, 100, 0, 90),
        note(1100, 100, 0, 90),
        note(1050, 500, 400, 90),
    ];
    assert_eq!(
        shape(&tidy(notes, &section)),
        [(1000, 100, 0), (1050, 500, 400), (1100, 100, 0)]
    );
    // A note ending exactly at the section's end is whole; one tick more is trimmed.
    assert_eq!(
        shape(&tidy(vec![note(1500, 500, 0, 90)], &section)),
        [(1500, 500, 0)]
    );
    assert_eq!(
        shape(&tidy(vec![note(1500, 501, 0, 90)], &section)),
        [(1500, 500, 0)]
    );
    assert!(tidy(vec![], &section).is_empty());
    assert!(tidy(vec![note(1500, 0, 0, 90), note(1600, -5, 0, 90)], &section).is_empty());
}

#[test]
fn t11_3_single_line_lets_only_one_note_sound() {
    let notes = vec![
        note(200, 500, 700, 90),
        note(0, 500, 0, 90),
        note(400, 100, 400, 90),
        note(400, 100, 900, 80),
    ];
    let line = single_line(notes);
    assert_eq!(
        shape(&line),
        [(0, 200, 0), (200, 200, 700), (400, 100, 400)]
    );
    // A note that already ends before the next keeps its length.
    assert_eq!(
        shape(&single_line(vec![note(0, 50, 0, 90), note(100, 50, 0, 90)])),
        [(0, 50, 0), (100, 50, 0)]
    );
    assert!(single_line(vec![]).is_empty());
}
