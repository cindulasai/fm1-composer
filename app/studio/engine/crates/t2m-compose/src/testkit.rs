//! Shared fixtures for this crate's tests.

use std::sync::OnceLock;

use t2m_core::brief::{
    Articulation, Brief, Register, Role, Section, SectionHarmony, TrackSpec, Transition,
};
use t2m_core::{Knowledge, repair};

pub(crate) fn kb() -> &'static Knowledge {
    static KB: OnceLock<Knowledge> = OnceLock::new();
    KB.get_or_init(|| Knowledge::load().expect("the embedded knowledge base loads"))
}

/// The gallery's first brief: four sections (4, 8, 8, 4 bars) and four tracks in C major.
pub(crate) fn starter() -> Brief {
    kb().gallery()[0].brief.clone()
}

/// The starter brief with an edit, repaired so that it is valid.
pub(crate) fn with(edit: impl FnOnce(&mut Brief)) -> Brief {
    let mut brief = starter();
    edit(&mut brief);
    repair(brief, kb()).0
}

pub(crate) fn progression(roman: &[&str], bars_per_chord: u8) -> SectionHarmony {
    SectionHarmony::Progression {
        roman: roman.iter().map(|symbol| symbol.to_string()).collect(),
        bars_per_chord,
    }
}

pub(crate) fn section(id: &str, bars: u16, tracks: &[&str]) -> Section {
    Section {
        id: id.into(),
        kind: "verse".into(),
        bars,
        energy: 0.5,
        density: 0.5,
        harmony: progression(&["I", "V", "vi", "IV"], 1),
        tracks: tracks.iter().map(|track| track.to_string()).collect(),
        transition: Transition::None,
    }
}

pub(crate) fn track(id: &str, role: Role, instrument: &str) -> TrackSpec {
    TrackSpec {
        id: id.into(),
        role,
        instrument: instrument.into(),
        register: Register::Mid,
        pattern: String::new(),
        busy: 0.5,
        articulation: Articulation::Normal,
    }
}
