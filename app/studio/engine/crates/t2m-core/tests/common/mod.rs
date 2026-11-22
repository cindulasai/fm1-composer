//! A knowledge base for tests: the embedded data plus entries that exercise
//! cycles, non-chordal systems, alternative tunings and several styles.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use t2m_core::Knowledge;

pub const EXTRAS: &[(&str, &str)] = &[
    (
        "tunings/extra.ron",
        r#"[
    (id: "5tet", cents: [0, 240, 480, 720, 960], period: 1200, source: "test fixture", verified: false),
    (id: "12well", cents: [0, 90, 196, 294, 390, 498, 588, 698, 792, 893, 996, 1092], period: 1200, source: "test fixture", verified: false),
]"#,
    ),
    (
        "tonal/extra.ron",
        r#"[
    (id: "raga.test", family: Raga, steps: [0, 2, 4, 6, 7, 9, 11], default_tuning: "12tet",
     tunings: ["12well"], aliases: ["evening raga"], chordable: false, source: "test fixture", verified: false),
    (id: "scale.five", family: Pentatonic, steps: [0, 1, 2, 3, 4], default_tuning: "5tet",
     chordable: false, source: "test fixture", verified: false),
    (id: "mode.phrygian", family: Mode, steps: [0, 1, 3, 5, 7, 8, 10], default_tuning: "12tet",
     tunings: ["12well"], chordable: true, source: "test fixture", verified: false),
]"#,
    ),
    (
        "cycles/extra.ron",
        r#"[
    (id: "tala.rupak", beats: 7, beat_unit: 4, grouping: [3, 2, 2], source: "test fixture", verified: false),
    (id: "tala.teental", beats: 16, beat_unit: 4, grouping: [4, 4, 4, 4], source: "test fixture", verified: false),
]"#,
    ),
    (
        "grooves/extra.ron",
        r#"[
    (id: "groove.rupak", name: "Rupak theka", cycles: ["tala.rupak"], source: "test fixture", verified: false),
]"#,
    ),
    (
        "instruments/extra.ron",
        r#"[
    (id: "kit.tabla", name: "Tabla", gm_program: 0, low: 0, high: 127, polyphonic: true, roles: [percussion],
     kit: [(name: "na", note: 60), (name: "ge", note: 61)], source: "test fixture", verified: false),
    (id: "strings.sitar", name: "Sitar", gm_program: 104, low: 48, high: 84, polyphonic: false,
     roles: [lead, drone], source: "test fixture", verified: false),
    (id: "keys.rhodes", name: "Electric piano", aliases: ["rhodes", "e-piano"], gm_program: 4, low: 28, high: 103,
     polyphonic: true, roles: [lead, chords], source: "test fixture", verified: false),
]"#,
    ),
    (
        "progressions/extra.ron",
        r#"[
    (id: "prog.andalusian", name: "Andalusian cadence", roman: ["i", "bVII", "bVI", "V"], bars_per_chord: 1,
     source: "test fixture", verified: false),
]"#,
    ),
    (
        "styles/extra.ron",
        r#"[
    (id: "hiphop.lofi", name: "Lo-fi hip hop", aliases: ["lofi hiphop", "chillhop"], tags: ["mellow"],
     tempo: (min: 70, max: 95, default: 82), meter: (4, 4), feel: "groove.lofi_swing",
     system: "mode.dorian", harmonic_language: modal,
     form: [(id: "a", kind: "loop", bars: 8, energy: 0.4, density: 0.4),
            (id: "b", kind: "loop", bars: 8, energy: 0.6, density: 0.5, roles: [chords, drums])],
     progressions: ["prog.andalusian"],
     palette: [(role: chords, instrument: "keys.rhodes"), (role: drums, instrument: "kit.standard")],
     source: "test fixture", verified: false),
    (id: "raga.style", name: "Raga performance", tags: ["hindustani"],
     tempo: (min: 60, max: 200, default: 90), meter: (3, 4), feel: "groove.rupak",
     system: "raga.test", harmonic_language: drone,
     form: [(id: "alap", kind: "exposition", bars: 4, energy: 0.2, density: 0.2, roles: [lead, drone]),
            (id: "gat", kind: "composition", bars: 8, energy: 0.7, density: 0.6)],
     palette: [(role: lead, instrument: "strings.sitar"), (role: drone, instrument: "strings.sitar"),
               (role: percussion, instrument: "kit.tabla"), (role: lead, instrument: "strings.sitar")],
     source: "test fixture", verified: false),
]"#,
    ),
];

/// A throwaway copy of the embedded data directory.
pub struct TempKb {
    pub root: PathBuf,
}

impl TempKb {
    pub fn new() -> TempKb {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!(
            "t2m-kbx-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let root = std::env::temp_dir().join(unique);
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        copy_dir(&source, &root);
        // The Western styles arrived after these tests were written; they test a small
        // seed, so the larger one is taken out of the copy.
        for gone in [
            "styles/western.ron",
            "progressions/western.ron",
            "instruments/gm_extra.ron",
            "cycles/tala.ron",
            "tonal/thaat.ron",
            "cycles/timeline.ron",
            "grooves/timeline.ron",
        ] {
            let _ = fs::remove_file(root.join(gone));
        }
        TempKb { root }
    }

    pub fn write(&self, relative: &str, text: &str) -> &TempKb {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    pub fn load(&self) -> Knowledge {
        Knowledge::from_dir(&self.root).unwrap()
    }
}

impl Drop for TempKb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The embedded knowledge base plus [`EXTRAS`].
pub fn extended_kb() -> Knowledge {
    let temp = TempKb::new();
    for (path, text) in EXTRAS {
        temp.write(path, text);
    }
    temp.load()
}
