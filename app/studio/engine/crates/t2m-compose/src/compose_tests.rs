use proptest::prelude::*;
use t2m_core::brief::{Brief, Length, MeterSpec, Role, SectionHarmony, Transition};

use super::*;
use crate::testkit::{kb, progression, section, starter, track, with};

fn check(score: &Score) {
    assert_eq!(score.check(), Ok(()));
}

#[test]
fn t18_1_the_same_seed_gives_the_same_piece_and_another_seed_another() {
    let brief = starter();
    let a = compose(&brief, kb(), 42);
    assert_eq!(a, compose(&brief, kb(), 42));
    assert_ne!(a, compose(&brief, kb(), 43));
    check(&a);
    assert_eq!(a.meta.seed, 42);
    assert_eq!(a.meta.data_version, kb().data_version());
    assert_eq!(a.tracks.len(), brief.tracks.len());
    assert!(a.tracks.iter().all(|t| !t.notes.is_empty()));
    assert_eq!(a.sections.len(), brief.form.len());
}

#[test]
fn t18_2_changing_one_track_leaves_the_others_untouched() {
    let old = starter();
    let mut new = old.clone();
    new.tracks[0].busy = 0.95;
    let before = compose(&old, kb(), 7);
    let after = compose(&new, kb(), 7);
    assert_ne!(before.tracks[0], after.tracks[0]);
    for index in 1..old.tracks.len() {
        if old.tracks[index].role != Role::Counter {
            assert_eq!(before.tracks[index], after.tracks[index], "track {index}");
        }
    }
}

#[test]
fn t18_3_changing_one_sections_energy_changes_only_that_section() {
    let old = starter();
    let mut new = old.clone();
    new.form[1].energy = if old.form[1].energy > 0.5 { 0.1 } else { 0.95 };
    let before = compose(&old, kb(), 7);
    let after = compose(&new, kb(), 7);
    let (from, to) = (before.sections[1].start, before.sections[1].end);
    let mut changed = false;
    for (a, b) in before.tracks.iter().zip(&after.tracks) {
        let outside = |t: &Track| {
            t.notes
                .iter()
                .filter(|n| n.start < from || n.start >= to)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(outside(a), outside(b), "{}", a.id);
        changed |= a.notes != b.notes;
    }
    assert!(changed);
}

#[test]
fn t18_4_a_reroll_changes_one_part_of_one_section() {
    let brief = starter();
    let base = compose(&brief, kb(), 7);
    let mut rerolls = Rerolls::default();
    let first = &brief.form[1].id;
    for part in ["bass", "drums", "lead", "chords"] {
        let Some(index) = brief.tracks.iter().position(|t| t.id == part) else {
            continue;
        };
        rerolls.bump(part, first);
        let after = recompose(&brief, kb(), 7, &rerolls);
        let (from, to) = (base.sections[1].start, base.sections[1].end);
        for (i, (a, b)) in base.tracks.iter().zip(&after.tracks).enumerate() {
            let outside = |t: &Track| {
                t.notes
                    .iter()
                    .filter(|n| n.start < from || n.start >= to)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(outside(a), outside(b), "{part}: {}", a.id);
            if i != index
                && !(part == "drums" && a.id == "bass")
                && !(part == "lead" && a.role == Role::Counter)
            {
                assert_eq!(a, b, "{part} rerolled but {} changed", a.id);
            }
        }
        assert_ne!(base.tracks[index], after.tracks[index], "{part}");
        rerolls = Rerolls::default();
    }
}

#[test]
fn t18_5_tempo_is_metadata_for_a_straight_groove() {
    let old = starter();
    let mut new = old.clone();
    new.time.tempo_bpm = old.time.tempo_bpm + 20;
    let (a, b) = (compose(&old, kb(), 3), compose(&new, kb(), 3));
    assert_eq!(a.tracks, b.tracks);
    assert_eq!(b.meta.tempo_bpm, old.time.tempo_bpm + 20);
}

#[test]
fn t18_6_the_smallest_and_the_oddest_briefs_still_give_a_score() {
    // One bar, one track.
    let tiny = with(|b| {
        b.tracks = vec![track("lead", Role::Lead, "keys.piano")];
        b.form = vec![section("a", 1, &["lead"])];
        b.time.length = Length::Bars(1);
    });
    let score = compose(&tiny, kb(), 1);
    check(&score);
    assert!(!score.tracks[0].notes.is_empty());
    // A section naming no tracks is silent; the next one is not.
    let gap = with(|b| {
        b.tracks = vec![
            track("lead", Role::Lead, "keys.piano"),
            track("bass", Role::Bass, "bass.electric"),
        ];
        b.form = vec![
            section("a", 4, &["lead", "bass"]),
            section("b", 4, &[]),
            section("c", 4, &["lead", "bass"]),
        ];
        b.time.length = Length::Bars(12);
    });
    let score = compose(&gap, kb(), 1);
    check(&score);
    let (b, c) = (score.sections[1].clone(), score.sections[2].clone());
    for t in &score.tracks {
        assert!(
            t.notes
                .iter()
                .all(|n| n.start < b.start || n.start >= b.end),
            "silent section"
        );
        assert!(
            t.notes.iter().any(|n| n.start >= c.start),
            "{} plays after",
            t.id
        );
    }
    // Slow and fast tempos with a swung groove: nothing before zero, nothing without length.
    for tempo in [30, 300] {
        let swung = with(|b| {
            b.time.feel = "groove.jazz_ride".into();
            b.time.tempo_bpm = tempo;
        });
        let score = compose(&swung, kb(), 5);
        check(&score);
        assert!(
            score
                .tracks
                .iter()
                .flat_map(|t| &t.notes)
                .all(|n| n.start >= 0 && n.dur > 0)
        );
    }
}

#[test]
fn t18_7_a_brief_that_is_not_valid_is_repaired_not_refused() {
    let mut broken = starter();
    broken.identity.style = "no such style".into();
    broken.time.tempo_bpm = 4000;
    broken.tracks[0].instrument = "nonsense".into();
    broken.form[0].tracks.push("ghost".into());
    let score = compose(&broken, kb(), 1);
    check(&score);
    assert!(score.tracks.iter().any(|t| !t.notes.is_empty()));
    assert!(score.meta.tempo_bpm <= 300);
}

#[test]
fn t18_8_five_hundred_twelve_bars_and_sixteen_tracks() {
    let big = with(|b| {
        let roles = [
            Role::Drums,
            Role::Bass,
            Role::Chords,
            Role::Pad,
            Role::Lead,
            Role::Counter,
            Role::Arp,
            Role::Fx,
        ];
        b.tracks = (0..16)
            .map(|i| {
                let role = roles[i % roles.len()];
                let instrument = match role {
                    Role::Drums => "kit.standard",
                    Role::Bass => "bass.electric",
                    _ => "keys.piano",
                };
                track(&format!("t{i}"), role, instrument)
            })
            .collect();
        let ids: Vec<String> = b.tracks.iter().map(|t| t.id.clone()).collect();
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        b.form = (0..8)
            .map(|i| section(&format!("s{i}"), 64, &ids))
            .collect();
        b.time.length = Length::Bars(512);
    });
    let score = compose(&big, kb(), 1);
    check(&score);
    assert_eq!(score.end(), 512 * 3840);
    assert_eq!(score.tracks.len(), 16);
}

#[test]
fn t18_9_sixty_four_bars_of_eight_tracks_is_fast_in_a_release_build() {
    let brief = with(|b| {
        let roles = [
            Role::Drums,
            Role::Bass,
            Role::Chords,
            Role::Pad,
            Role::Lead,
            Role::Counter,
            Role::Arp,
            Role::Fx,
        ];
        b.tracks = roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                let instrument = match role {
                    Role::Drums => "kit.standard",
                    Role::Bass => "bass.electric",
                    _ => "keys.piano",
                };
                track(&format!("t{i}"), *role, instrument)
            })
            .collect();
        let ids: Vec<String> = b.tracks.iter().map(|t| t.id.clone()).collect();
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        b.form = (0..4)
            .map(|i| section(&format!("s{i}"), 16, &ids))
            .collect();
        b.time.length = Length::Bars(64);
    });
    let started = std::time::Instant::now();
    let score = compose(&brief, kb(), 1);
    let took = started.elapsed();
    check(&score);
    // Only a release build is held to the budget; a debug build is far slower.
    if !cfg!(debug_assertions) {
        assert!(took.as_millis() <= 150, "took {took:?}");
    }
}

fn any_brief() -> impl Strategy<Value = Brief> {
    let roles = prop::sample::select(vec![
        Role::Drums,
        Role::Percussion,
        Role::Bass,
        Role::Chords,
        Role::Pad,
        Role::Lead,
        Role::Counter,
        Role::Arp,
        Role::Drone,
        Role::Fx,
    ]);
    let tracks = prop::collection::vec((roles, 0.0f64..=1.0, 0u8..5), 1..6);
    let form = prop::collection::vec(
        (
            1u16..7,
            0.0f64..=1.0,
            0.0f64..=1.0,
            0u8..4,
            0u8..7,
            any::<u8>(),
        ),
        1..5,
    );
    let feel = prop::sample::select(kb().grooves().map(|g| g.id.clone()).collect::<Vec<_>>());
    let meter = prop::sample::select(vec![
        (4u8, 4u8, None),
        (3, 4, None),
        (6, 8, None),
        (5, 4, None),
        (7, 8, Some(vec![2u8, 2, 3])),
    ]);
    (tracks, form, feel, meter, 10u16..400, any::<bool>()).prop_map(
        |(tracks, form, feel, meter, tempo, _)| {
            with(|b| {
                b.time.feel = feel;
                b.time.tempo_bpm = tempo;
                b.time.meter = MeterSpec::Meter {
                    numerator: meter.0,
                    denominator: meter.1,
                    grouping: meter.2,
                };
                b.tracks = tracks
                    .iter()
                    .enumerate()
                    .map(|(i, (role, busy, register))| {
                        let instrument = match role {
                            Role::Drums | Role::Percussion => "kit.standard",
                            Role::Bass => "bass.electric",
                            _ => "keys.piano",
                        };
                        let mut t = track(&format!("t{i}"), *role, instrument);
                        t.busy = *busy;
                        t.register = [
                            t2m_core::brief::Register::Low,
                            t2m_core::brief::Register::MidLow,
                            t2m_core::brief::Register::Mid,
                            t2m_core::brief::Register::MidHigh,
                            t2m_core::brief::Register::High,
                        ][usize::from(*register)];
                        t
                    })
                    .collect();
                let ids: Vec<String> = b.tracks.iter().map(|t| t.id.clone()).collect();
                b.form = form
                    .iter()
                    .enumerate()
                    .map(|(i, (bars, energy, density, harmony, transition, mask))| {
                        let on: Vec<&str> = ids
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| (mask >> (j % 8)) & 1 == 1 || *j == 0)
                            .map(|(_, id)| id.as_str())
                            .collect();
                        let mut s = section(&format!("s{i}"), *bars, &on);
                        s.energy = *energy;
                        s.density = *density;
                        s.harmony = match harmony {
                            0 => progression(&["I", "V", "vi", "IV"], 1),
                            1 => progression(&["i", "bVII", "bVI", "V7"], 2),
                            2 => SectionHarmony::Drone {
                                degrees: vec![1, 5],
                            },
                            _ => SectionHarmony::None,
                        };
                        s.transition = [
                            Transition::None,
                            Transition::FillLight,
                            Transition::FillHeavy,
                            Transition::Crash,
                            Transition::Riser,
                            Transition::Impact,
                            Transition::Break,
                        ][usize::from(*transition)];
                        s
                    })
                    .collect();
                b.time.length = Length::Bars(b.form.iter().map(|s| s.bars).sum());
            })
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn t18_10_any_repaired_brief_composes_into_a_playable_score(brief in any_brief(), seed in any::<u64>()) {
        let score = compose(&brief, kb(), seed);
        prop_assert_eq!(score.check(), Ok(()));
        prop_assert_eq!(compose(&brief, kb(), seed), score);
    }
}

#[test]
fn t50_3_a_whole_clave_piece_composes_and_passes_the_checks_and_the_critic() {
    use t2m_core::brief::{Length, MeterSpec};
    for id in [
        "timeline.son_3_2",
        "timeline.rumba_2_3",
        "timeline.bell_12_8",
    ] {
        let mut brief = starter();
        brief.time.feel = "groove.timeline".into();
        brief.time.meter = MeterSpec::Cycle { id: id.into() };
        brief.time.length = Length::Cycles(8);
        let (brief, _) = t2m_core::repair(brief, kb());
        assert_eq!(t2m_core::validate(&brief, kb()), Ok(()), "{id}");
        assert_eq!(
            brief.time.meter,
            MeterSpec::Cycle { id: id.into() },
            "repair kept the cycle"
        );
        let score = compose(&brief, kb(), 3);
        check(&score);
        let drums = score
            .tracks
            .iter()
            .find(|t| t.role == Role::Drums)
            .expect("the starter has drums");
        assert!(!drums.notes.is_empty(), "{id}");
        assert!(score.end() > 0);
        let card = critique(&score, &brief, kb());
        assert!(
            card.hard_failures.is_empty(),
            "{id}: {:?}",
            card.hard_failures
        );
    }
}

#[test]
fn t52_3_every_thaat_composes_a_valid_piece_with_only_its_own_notes() {
    for id in [
        "thaat.bilaval",
        "thaat.kalyan",
        "thaat.khamaj",
        "thaat.bhairav",
        "thaat.kafi",
        "thaat.asavari",
        "thaat.bhairavi",
        "thaat.poorvi",
        "thaat.marva",
        "thaat.todi",
    ] {
        let mut brief = starter();
        brief.tonality.system = id.into();
        let (brief, _) = t2m_core::repair(brief, kb());
        assert_eq!(brief.tonality.system, id, "repair kept the system");
        assert_eq!(t2m_core::validate(&brief, kb()), Ok(()), "{id}");
        let score = compose(&brief, kb(), 11);
        check(&score);
        let card = critique(&score, &brief, kb());
        assert!(
            card.hard_failures.is_empty(),
            "{id}: {:?}",
            card.hard_failures
        );
    }
}
