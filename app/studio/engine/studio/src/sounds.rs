//! Which FM-1 sound a part plays, and its role when its track name does not say: a copy of
//! app/tools/fm1c/sounds.py (the one table; the tests compare the bytes both make). Sound ids are the firmware's.

pub const NAMES: [&str; 138] = ["STD KIT", "TIGHT KIT", "BOOM KIT", "SAW LEAD", "SOFT PAD", "SQR BASS", "PWM STR", "ACID",
    "SINE KEY", "RAVE", "SUB BASS", "PLUCK", "BRASS", "WIND", "STRINGS", "BRASS",
    "ORGAN", "STRING", "RESO", "BELL", "WIRE", "FAT BASS", "ARP LEAD", "SYNC LEAD",
    "RING BELL", "CHIP CHOIR", "FULL ORGAN", "JAZZ PERC", "GOSPEL", "SOFT FLUTE", "ROCK DRIVE", "PULSE LD",
    "WAVE BASS", "ARP 8BIT", "WAVE LEAD", "STEP LEAD", "WIND", "RAIN", "ARCADE", "METAL",
    "808", "909", "606", "80S", "VINTAGE", "TRAP", "DRILL", "BOOMBAP",
    "LO-FI", "PHONK", "HOUSE", "D.HOUSE", "TECHNO", "MINIMAL", "ELECTRO", "DISCO",
    "GARAGE", "JUNGLE", "DUBSTEP", "DEMBOW", "AMAPIANO", "AFRO", "LATIN", "TRIBAL",
    "SYNTHWV", "CHIP", "ARCADE", "GLITCH", "INDUSTR", "HYPER", "AMBIENT", "JAZZ",
    "TINE EP", "GLASS BELL", "ROUND BASS", "BRASS SECT", "SOFT PAD", "WOOD BARS", "DRAWBARS", "NYLON PICK",
    "BELL TREE", "MODAL BAR", "STR PLUCK", "BOWED MTL", "THUMB PNO", "HAND DRUM", "MEMB TOMS", "SITAR",
    "PHYS HARP", "NYLON GTR", "STEEL GTR", "MUTED GTR", "PLUCK BASS", "KOTO", "BANJO", "CELLO BOW", "TANPURA",
    "CHIMES", "BIG BELL", "VIBRA BAR", "GLASS BOWL", "STEEL PAN", "WOOD BLOCK", "TABLA", "CONGA", "TIMPANI",
    "RHODES", "DX RHODES", "WURLI", "CLAV", "M1 PIANO", "AFRO KEYS", "TRAP BELL", "MUSIC BOX", "MARIMBA",
    "KALIMBA", "PLUGG BELL", "GLASS PAD", "FM BASS", "STEEL DRUM", "GLOCKEN", "CELESTA", "XYLOPHONE", "HARP",
    "BASS GTR", "TUBE BELL", "FM PLUCK", "FM GRAND", "CHOIR AAH", "SOUL OOH", "TALKBOX", "WOW BASS",
    "HARMONICA", "ROBOT", "VOX PAD", "PIANO", "STRINGS", "HORNS"];
pub const FM6_FIRST: u8 = 72;
pub const FM6_POOL: usize = 3;
pub const FM6_ALT: [u8; 8] = [8, 19, 21, 12, 4, 24, 16, 11];

const GM: [u8; 128] = [135, 135, 135, 135, 135, 135, 135, 135,
    73, 73, 24, 77, 77, 77, 19, 11,
    78, 78, 26, 16, 28, 28, 13, 28,
    79, 79, 11, 11, 11, 30, 30, 24,
    74, 21, 21, 74, 5, 5, 10, 32,
    136, 136, 136, 136, 136, 136, 136, 136,
    136, 136, 136, 136, 25, 25, 25, 15,
    137, 137, 137, 137, 137, 137, 137, 137,
    13, 13, 13, 13, 13, 13, 13, 29,
    29, 29, 29, 29, 29, 29, 29, 29,
    31, 3, 29, 34, 30, 25, 23, 7,
    76, 4, 6, 25, 17, 20, 76, 18,
    37, 4, 73, 4, 19, 38, 24, 38,
    20, 11, 11, 79, 77, 13, 17, 13,
    24, 27, 77, 27, 74, 74, 39, 36,
    39, 36, 37, 38, 38, 36, 37, 39];
const GM_ROLE: [&str; 16] = ["chords", "chords", "chords", "chords", "bass", "pad", "pad", "lead",
    "lead", "lead", "lead", "pad", "fx", "lead", "fx", "fx"];
const DRUM_KITS: [u8; 8] = [0, 44, 2, 41, 71, 71, 0, 67];
const TR808: u8 = 25;

const INSTRUMENTS: [(&str, u8, &str); 15] = [
    ("keys.piano", 0, "chords"), ("bass.electric", 33, "bass"), ("kit.standard", 0, "drums"),
    ("keys.rhodes", 4, "chords"), ("pad.warm", 89, "pad"), ("synth.lead", 80, "lead"), ("bass.synth", 38, "bass"),
    ("bass.upright", 32, "bass"), ("guitar.clean", 27, "chords"), ("guitar.overdrive", 29, "chords"),
    ("guitar.nylon", 24, "chords"), ("strings.ensemble", 48, "pad"), ("harp.orchestral", 46, "arp"),
    ("vibes.vibraphone", 11, "chords"), ("fx.riser", 119, "fx"),
];
const ALIASES: [(&str, &str); 56] = [
    ("acoustic grand piano", "keys.piano"), ("piano", "keys.piano"), ("grand piano", "keys.piano"),
    ("acoustic piano", "keys.piano"),
    ("electric bass (finger)", "bass.electric"), ("bass", "bass.electric"), ("electric bass", "bass.electric"),
    ("bass guitar", "bass.electric"), ("fingered bass", "bass.electric"),
    ("standard drum kit", "kit.standard"), ("drums", "kit.standard"), ("drum kit", "kit.standard"), ("kit", "kit.standard"),
    ("acoustic drums", "kit.standard"),
    ("electric piano", "keys.rhodes"), ("rhodes", "keys.rhodes"), ("e-piano", "keys.rhodes"), ("wurli", "keys.rhodes"),
    ("warm pad", "pad.warm"), ("synth pad", "pad.warm"), ("pad", "pad.warm"),
    ("square synth lead", "synth.lead"), ("synth lead", "synth.lead"), ("square lead", "synth.lead"),
    ("lead synth", "synth.lead"), ("synth", "synth.lead"),
    ("synth bass", "bass.synth"), ("808 bass", "bass.synth"), ("sub bass", "bass.synth"),
    ("acoustic bass", "bass.upright"), ("upright bass", "bass.upright"), ("double bass", "bass.upright"),
    ("standup bass", "bass.upright"),
    ("clean electric guitar", "guitar.clean"), ("clean guitar", "guitar.clean"), ("electric guitar", "guitar.clean"),
    ("guitar", "guitar.clean"),
    ("overdriven guitar", "guitar.overdrive"), ("distorted guitar", "guitar.overdrive"), ("rock guitar", "guitar.overdrive"),
    ("nylon-string guitar", "guitar.nylon"), ("nylon guitar", "guitar.nylon"), ("classical guitar", "guitar.nylon"),
    ("spanish guitar", "guitar.nylon"),
    ("string ensemble", "strings.ensemble"), ("strings", "strings.ensemble"), ("orchestral strings", "strings.ensemble"),
    ("string section", "strings.ensemble"),
    ("orchestral harp", "harp.orchestral"), ("harp", "harp.orchestral"),
    ("vibraphone", "vibes.vibraphone"), ("vibes", "vibes.vibraphone"),
    ("reverse cymbal riser", "fx.riser"), ("riser", "fx.riser"), ("sweep", "fx.riser"), ("reverse cymbal", "fx.riser"),
];

/// the text2midi instrument a track name names (its program and role): after "<role> – ", or the whole name
fn instrument(name: &str) -> Option<(u8, &'static str)> {
    let s = name.split_once(" \u{2013} ").map_or(name, |x| x.1).to_lowercase();
    let id = ALIASES.iter().find(|(a, _)| *a == s).map_or(s.as_str(), |(_, id)| id);
    INSTRUMENTS.iter().find(|(i, _, _)| *i == id).map(|&(_, p, r)| (p, r))
}

/// the role of a part whose name is not a role: its instrument's, else its program family's
pub fn role_hint(name: &str, program: Option<u8>) -> Option<&'static str> {
    if let Some((_, role)) = instrument(name) { return Some(role); }
    program.filter(|&p| p <= 127).map(|p| GM_ROLE[p as usize / 8])
}

/// the sound a part plays; None when nothing names one (the firmware then plays the role's default)
pub fn sound(name: &str, program: Option<u8>, role: &str) -> Option<u8> {
    let drums = role == "drums" || role == "percussion";
    let program = match instrument(name) {
        Some((p, r)) if (r == "drums") == drums => p,
        _ => program.filter(|&p| p <= 127)?,
    };
    if !drums { return Some(GM[program as usize]); }
    if program == TR808 { return Some(40); }
    Some(if program < 64 { DRUM_KITS[program as usize / 8] } else { 0 })
}
