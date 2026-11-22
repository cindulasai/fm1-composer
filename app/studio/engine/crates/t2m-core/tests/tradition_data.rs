use t2m_core::Knowledge;

/// (id, beats, grouping, [(beat, weight)]) copied from the reference page's table, independently of the data file.
type Row = (&'static str, u8, &'static [u8], &'static [(u8, u8)]);
const TALAS: [Row; 7] = [
    (
        "tala.tintal",
        16,
        &[4, 4, 4, 4],
        &[(0, 3), (4, 2), (8, 1), (12, 2)],
    ),
    (
        "tala.jhaptal",
        10,
        &[2, 3, 2, 3],
        &[(0, 3), (2, 2), (5, 1), (7, 2)],
    ),
    ("tala.rupak", 7, &[3, 2, 2], &[(0, 3), (3, 2), (5, 2)]),
    (
        "tala.ektal",
        12,
        &[2, 2, 2, 2, 2, 2],
        &[(0, 3), (2, 1), (4, 2), (6, 1), (8, 2), (10, 2)],
    ),
    ("tala.dadra", 6, &[3, 3], &[(0, 3), (3, 1)]),
    ("tala.keherwa", 8, &[4, 4], &[(0, 3), (4, 1)]),
    (
        "tala.dhamar",
        14,
        &[5, 2, 3, 4],
        &[(0, 3), (5, 2), (7, 1), (10, 2)],
    ),
];

#[test]
fn t48_1_the_seven_talas_match_the_reference_table() {
    let kb = Knowledge::load().unwrap();
    for (id, beats, grouping, accents) in TALAS {
        let cycle = kb.cycle(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!((cycle.beats, cycle.beat_unit), (beats, 4), "{id}");
        assert_eq!(cycle.grouping, grouping, "{id} grouping");
        let got: Vec<(u8, u8)> = cycle.accents.iter().map(|a| (a.beat, a.weight)).collect();
        assert_eq!(got, accents, "{id} accents");
        assert_eq!(
            cycle.grouping.iter().map(|g| u16::from(*g)).sum::<u16>(),
            u16::from(beats)
        );
        // Accents sit exactly on group starts, and the sam is the first beat and the strongest.
        let mut starts = vec![0u8];
        for g in &cycle.grouping[..cycle.grouping.len() - 1] {
            starts.push(starts.last().unwrap() + g);
        }
        assert_eq!(
            cycle.accents.iter().map(|a| a.beat).collect::<Vec<_>>(),
            starts,
            "{id}"
        );
        assert_eq!(cycle.accents[0].weight, 3);
        assert!(
            cycle.verified && cycle.source.contains("wikipedia.org/wiki/Tala_(music)"),
            "{id}"
        );
    }
}

#[test]
fn t48_2_the_knowledge_base_has_no_unverified_or_unsourced_cycles() {
    let kb = Knowledge::load().unwrap();
    for id in TALAS.map(|t| t.0) {
        let c = kb.cycle(id).unwrap();
        assert!(c.verified && !c.source.trim().is_empty());
    }
    assert!(kb.cycle("tala.nope").is_none());
}

/// (id, steps, beat unit, onset steps counted from 1, page pattern or derivation).
const TIMELINES: [(&str, u8, u8, &[u8]); 5] = [
    ("timeline.son_3_2", 16, 16, &[1, 4, 7, 11, 13]),
    ("timeline.son_2_3", 16, 16, &[3, 5, 9, 12, 15]),
    ("timeline.rumba_3_2", 16, 16, &[1, 4, 8, 11, 13]),
    ("timeline.rumba_2_3", 16, 16, &[3, 5, 9, 12, 16]),
    ("timeline.bell_12_8", 12, 8, &[1, 3, 5, 6, 8, 10, 12]),
];

#[test]
fn t49_1_timelines_have_exactly_the_reference_onsets_one_stroke_per_step() {
    let kb = Knowledge::load().unwrap();
    for (id, steps, unit, onsets) in TIMELINES {
        let cycle = kb.cycle(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!((cycle.beats, cycle.beat_unit), (steps, unit), "{id}");
        assert_eq!(cycle.strokes.len(), usize::from(steps), "{id}");
        let got: Vec<u8> = cycle
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name == "x")
            .map(|(i, _)| i as u8 + 1)
            .collect();
        assert_eq!(got, onsets, "{id} onsets");
        assert!(
            cycle.strokes.iter().all(|s| s.name == "x" || s.name == "."),
            "{id}"
        );
        assert_eq!(
            cycle.grouping.iter().map(|g| u16::from(*g)).sum::<u16>(),
            u16::from(steps)
        );
        assert!(
            cycle.verified && cycle.source.contains("wikipedia.org/wiki/Clave_(rhythm)"),
            "{id}"
        );
    }
}

#[test]
fn t49_2_two_three_is_the_three_two_with_its_halves_swapped_and_bossa_is_absent() {
    let kb = Knowledge::load().unwrap();
    let onsets = |id: &str| -> Vec<u8> {
        kb.cycle(id)
            .unwrap()
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name == "x")
            .map(|(i, _)| i as u8 + 1)
            .collect()
    };
    for kind in ["son", "rumba"] {
        let a = onsets(&format!("timeline.{kind}_3_2"));
        let mut swapped: Vec<u8> = a
            .iter()
            .map(|s| if *s <= 8 { s + 8 } else { s - 8 })
            .collect();
        swapped.sort_unstable();
        assert_eq!(onsets(&format!("timeline.{kind}_2_3")), swapped, "{kind}");
    }
    // The page gives no onset grid for bossa, so none is stored.
    assert!(kb.cycle("timeline.bossa").is_none() && kb.cycle("timeline.bossa_3_2").is_none());
}

/// (id, the page's "Western notes in C" column).
const THAATS: [(&str, &str); 10] = [
    ("thaat.bilaval", "C D E F G A B"),
    ("thaat.kalyan", "C D E F# G A B"),
    ("thaat.khamaj", "C D E F G A Bb"),
    ("thaat.bhairav", "C Db E F G Ab B"),
    ("thaat.kafi", "C D Eb F G A Bb"),
    ("thaat.asavari", "C D Eb F G Ab Bb"),
    ("thaat.bhairavi", "C Db Eb F G Ab Bb"),
    ("thaat.poorvi", "C Db E F# G Ab B"),
    ("thaat.marva", "C Db E F# G A B"),
    ("thaat.todi", "C Db Eb F# G Ab B"),
];

fn semitone(name: &str) -> u8 {
    let mut chars = name.chars();
    let base = match chars.next().unwrap() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        other => panic!("{other}"),
    };
    chars.fold(base, |v, c| if c == '#' { v + 1 } else { v - 1 })
}

#[test]
fn t52_1_the_ten_thaats_equal_the_pages_western_notes_in_c() {
    let kb = Knowledge::load().unwrap();
    for (id, notes) in THAATS {
        let system = kb
            .tonal_system(id)
            .unwrap_or_else(|| panic!("{id} missing"));
        let want: Vec<u8> = notes.split(' ').map(semitone).collect();
        assert_eq!(system.steps, want, "{id}");
        assert_eq!(system.default_tuning, "12tet", "{id}");
        assert!(
            system.verified && system.source.contains("wikipedia.org/wiki/Thaat"),
            "{id}"
        );
        assert_eq!(system.family, t2m_core::TonalFamily::Raga);
    }
    assert_eq!(THAATS.len(), 10);
}

#[test]
fn t52_2_thaat_names_and_spellings_find_the_system() {
    let kb = Knowledge::load().unwrap();
    assert_eq!(
        kb.nearest(t2m_core::Kind::TonalSystem, "marwa"),
        Some("thaat.marva")
    );
    assert_eq!(
        kb.nearest(t2m_core::Kind::TonalSystem, "Bilawal"),
        Some("thaat.bilaval")
    );
    assert_eq!(
        kb.nearest(t2m_core::Kind::TonalSystem, "thaat bhairav"),
        Some("thaat.bhairav")
    );
}
