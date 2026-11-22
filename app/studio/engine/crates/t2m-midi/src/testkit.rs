//! Scores and helpers shared by the tests of this crate.

use t2m_core::brief::Role;
use t2m_core::score::{BendCurve, ScoreMeta, SectionSpan, Track};
use t2m_core::{Knowledge, Meter, Note, Pitch, PitchClass, Score, TimeFrame, Tonic};

pub fn kb() -> Knowledge {
    Knowledge::load().unwrap()
}

pub fn note(start: i64, dur: i64, cents: i32) -> Note {
    Note {
        start,
        dur,
        pitch: Pitch { cents },
        vel: 90,
        bend: None,
    }
}

pub fn bent(start: i64, dur: i64, cents: i32, curve: &[(i64, i32)]) -> Note {
    Note {
        bend: Some(BendCurve(curve.to_vec())),
        ..note(start, dur, cents)
    }
}

pub fn track(id: &str, role: Role, instrument: &str, notes: Vec<Note>) -> Track {
    Track {
        id: id.into(),
        role,
        instrument: instrument.into(),
        notes,
    }
}

/// A 4/4, 120 bpm, tonic-C score with two 4-bar sections.
pub fn score(tracks: Vec<Track>) -> Score {
    Score {
        meta: ScoreMeta {
            tempo_bpm: 120,
            frame: TimeFrame::Meter(Meter::new(4, 4, None).unwrap()),
            tonic: Tonic::Class(PitchClass::C),
            tuning_id: "12tet".into(),
            data_version: "0123456789abcdef".into(),
            seed: 1,
        },
        tracks,
        sections: vec![
            SectionSpan {
                id: "a".into(),
                start: 0,
                end: 15_360,
            },
            SectionSpan {
                id: "b".into(),
                start: 15_360,
                end: 30_720,
            },
        ],
    }
}

pub fn tonic_key(score: &Score) -> i32 {
    i32::from(score.meta.tonic.midi_and_offset(0).0)
}
