use t2m_core::brief::Role;
use t2m_core::score::{BendCurve, Note, Score, ScoreIssue, ScoreMeta, SectionSpan, Track};
use t2m_core::{Meter, Pitch, PitchClass, TimeFrame, Tonic};

fn note(start: i64, dur: i64, cents: i32) -> Note {
    Note {
        start,
        dur,
        pitch: Pitch { cents },
        vel: 90,
        bend: None,
    }
}

fn span(id: &str, start: i64, end: i64) -> SectionSpan {
    SectionSpan {
        id: id.into(),
        start,
        end,
    }
}

fn score(notes: Vec<Note>) -> Score {
    Score {
        meta: ScoreMeta {
            tempo_bpm: 120,
            frame: TimeFrame::Meter(Meter::new(4, 4, None).unwrap()),
            tonic: Tonic::Class(PitchClass::D),
            tuning_id: "12tet".into(),
            data_version: "0123456789abcdef".into(),
            seed: 42,
        },
        tracks: vec![Track {
            id: "lead".into(),
            role: Role::Lead,
            instrument: "keys.piano".into(),
            notes,
        }],
        sections: vec![span("a", 0, 3840), span("b", 3840, 7680)],
    }
}

fn issues(score: &Score) -> Vec<ScoreIssue> {
    score.check().err().unwrap_or_default()
}

fn lead(note: usize) -> (String, usize) {
    ("lead".to_string(), note)
}

#[test]
fn t9_1_a_valid_score_passes() {
    let two = score(vec![note(0, 960, 0), note(960, 960, 700)]);
    assert_eq!(two.check(), Ok(()));
    assert_eq!(two.end(), 7680);
    assert_eq!(two.tracks[0].notes[1].end(), 1920);
    // Notes may fill the form exactly, start together, and use the full velocity range.
    let mut full = score(vec![
        note(0, 7680, 0),
        note(0, 7680, 400),
        note(7679, 1, 700),
    ]);
    full.tracks[0].notes[0].vel = 1;
    full.tracks[0].notes[1].vel = 127;
    assert_eq!(full.check(), Ok(()));
}

#[test]
fn t9_2_each_rule_fails_alone_with_its_own_issue() {
    let (track, at) = lead(1);
    let unsorted = score(vec![note(960, 100, 0), note(0, 100, 700)]);
    assert_eq!(
        issues(&unsorted),
        [ScoreIssue::Unsorted {
            track: track.clone(),
            note: at
        }]
    );

    for dur in [0, -1, i64::MIN] {
        let silent = score(vec![note(0, 100, 0), note(960, dur, 700)]);
        assert_eq!(
            issues(&silent),
            [ScoreIssue::Duration {
                track: track.clone(),
                note: at
            }],
            "dur {dur}"
        );
    }
    assert_eq!(issues(&score(vec![note(0, 1, 0)])), []);

    for vel in [0, 128, 255] {
        let mut loud = score(vec![note(0, 100, 0), note(960, 100, 700)]);
        loud.tracks[0].notes[1].vel = vel;
        assert_eq!(
            issues(&loud),
            [ScoreIssue::Velocity {
                track: track.clone(),
                note: at
            }],
            "vel {vel}"
        );
    }

    let overlap = score(vec![note(0, 961, 0), note(960, 100, 0)]);
    assert_eq!(
        issues(&overlap),
        [ScoreIssue::Overlap {
            track: track.clone(),
            note: at
        }]
    );
    // A long note is still sounding when a third note of its pitch arrives after a short one.
    let hidden = score(vec![
        note(0, 3000, 0),
        note(1000, 10, 700),
        note(2000, 10, 0),
    ]);
    assert_eq!(
        issues(&hidden),
        [ScoreIssue::Overlap {
            track: track.clone(),
            note: 2
        }]
    );
    let nested = score(vec![note(0, 3000, 0), note(100, 10, 0), note(500, 10, 0)]);
    assert_eq!(
        issues(&nested),
        [
            ScoreIssue::Overlap {
                track: track.clone(),
                note: 1
            },
            ScoreIssue::Overlap {
                track: track.clone(),
                note: 2
            }
        ]
    );

    let late = score(vec![note(0, 100, 0), note(7000, 681, 700)]);
    assert_eq!(
        issues(&late),
        [ScoreIssue::OutsideForm {
            track: track.clone(),
            note: at
        }]
    );
    assert_eq!(issues(&score(vec![note(7000, 680, 700)])), []);
    let endless = score(vec![note(100, i64::MAX, 700)]);
    assert_eq!(
        issues(&endless),
        [ScoreIssue::OutsideForm {
            track: track.clone(),
            note: 0
        }]
    );

    let early = score(vec![note(-1, 100, 0)]);
    assert_eq!(
        issues(&early),
        [ScoreIssue::NegativeStart {
            track: track.clone(),
            note: 0
        }]
    );
    assert_eq!(issues(&score(vec![note(0, 100, 0)])), []);

    // Several problems are all reported, in note order.
    let mut many = score(vec![note(960, 0, 0), note(-5, 100, 700)]);
    many.tracks[0].notes[0].vel = 0;
    assert_eq!(
        issues(&many),
        [
            ScoreIssue::Duration {
                track: track.clone(),
                note: 0
            },
            ScoreIssue::Velocity {
                track: track.clone(),
                note: 0
            },
            ScoreIssue::Unsorted {
                track: track.clone(),
                note: 1
            },
            ScoreIssue::NegativeStart {
                track: track.clone(),
                note: 1
            },
        ]
    );
    // Each track is checked by itself.
    let mut two = score(vec![note(0, 100, 0)]);
    let mut bass = two.tracks[0].clone();
    bass.id = "bass".into();
    bass.notes = vec![note(50, 100, 0), note(60, 0, -1200)];
    two.tracks.push(bass);
    assert_eq!(
        issues(&two),
        [ScoreIssue::Duration {
            track: "bass".into(),
            note: 1
        }]
    );
    assert_eq!(
        ScoreIssue::Duration {
            track: "bass".into(),
            note: 1
        }
        .to_string(),
        "track bass, note 1: has no length"
    );
}

#[test]
fn t9_3_touching_notes_and_different_pitches_are_not_overlaps() {
    let touching = score(vec![note(0, 960, 0), note(960, 960, 0), note(1920, 960, 0)]);
    assert_eq!(touching.check(), Ok(()));
    let chord = score(vec![
        note(0, 3840, 0),
        note(0, 3840, 400),
        note(100, 3000, 700),
        note(200, 10, 1),
    ]);
    assert_eq!(chord.check(), Ok(()));
    // The same pitch on two tracks is not an overlap either.
    let mut two = score(vec![note(0, 3840, 0)]);
    let mut other = two.tracks[0].clone();
    other.id = "pad".into();
    two.tracks.push(other);
    assert_eq!(two.check(), Ok(()));
}

#[test]
fn t9_4_scores_round_trip_through_json() {
    let mut bent = score(vec![note(0, 960, 350), note(960, 960, 700)]);
    bent.tracks[0].notes[0].bend = Some(BendCurve(vec![(0, -50), (480, 0), (900, 25)]));
    bent.meta.tonic = Tonic::Hz(146.83);
    let json = serde_json::to_string(&bent).unwrap();
    let back: Score = serde_json::from_str(&json).unwrap();
    assert_eq!(back, bent);
    assert_eq!(
        back.tracks[0].notes[0].bend.as_ref().unwrap().0[2],
        (900, 25)
    );
    // A note written without a bend reads as having none.
    let bare: Note =
        serde_json::from_str(r#"{"start":0,"dur":10,"pitch":{"cents":0},"vel":64}"#).unwrap();
    assert_eq!(bare.bend, None);
}

#[test]
fn t9_5_empty_scores_and_section_order() {
    let mut empty = score(vec![]);
    empty.tracks.clear();
    empty.sections.clear();
    assert_eq!(empty.check(), Ok(()));
    assert_eq!(empty.end(), 0);
    // With no sections there is no form for a note to be inside.
    let mut formless = score(vec![note(0, 10, 0)]);
    formless.sections.clear();
    assert_eq!(
        issues(&formless),
        [ScoreIssue::OutsideForm {
            track: "lead".into(),
            note: 0
        }]
    );

    let sections = |spans: Vec<SectionSpan>| {
        let mut score = score(vec![]);
        score.sections = spans;
        issues(&score)
    };
    assert_eq!(sections(vec![span("a", 0, 100), span("b", 100, 200)]), []);
    // A gap between sections is allowed; the form may start late.
    assert_eq!(sections(vec![span("a", 50, 100), span("b", 150, 200)]), []);
    assert_eq!(
        sections(vec![span("a", 0, 100), span("b", 99, 200)]),
        [ScoreIssue::Sections { section: 1 }]
    );
    assert_eq!(
        sections(vec![span("a", 100, 200), span("b", 0, 100)]),
        [ScoreIssue::Sections { section: 1 }]
    );
    assert_eq!(
        sections(vec![span("a", 0, 0)]),
        [ScoreIssue::Sections { section: 0 }]
    );
    assert_eq!(
        sections(vec![span("a", 100, 50)]),
        [ScoreIssue::Sections { section: 0 }]
    );
    assert_eq!(
        sections(vec![span("a", -10, 50)]),
        [ScoreIssue::Sections { section: 0 }]
    );
    // A later section is compared with the furthest end seen so far.
    assert_eq!(
        sections(vec![
            span("a", 0, 500),
            span("b", 100, 200),
            span("c", 300, 600)
        ]),
        [
            ScoreIssue::Sections { section: 1 },
            ScoreIssue::Sections { section: 2 }
        ]
    );
    assert_eq!(
        ScoreIssue::Sections { section: 2 }.to_string(),
        "section 2: is empty, or begins before the one in front of it ends"
    );
}
