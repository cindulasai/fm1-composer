//! The .f1p piece format v1, as app/tools/fm1c/piece.py writes it (firmware/src/piece.h reads it).
use unicode_normalization::UnicodeNormalization;

pub const PPQ: u32 = 96;
pub const MAX_BYTES: usize = 32768;
pub const MAX_CH: usize = 8;
pub const MAX_TICKS: u32 = 96 * 4 * 999;
pub const NAME_LEN: usize = 24;
pub const ROLES: [&str; 11] = ["lead", "counter", "chords", "pad", "bass", "drums", "percussion", "arp", "drone", "fx", "other"];
const REST: u8 = 255;

/// Ordered as piece.py's Note: tick, pitch, dur, vel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Note { pub tick: u32, pub pitch: u8, pub dur: u16, pub vel: u8 }

#[derive(Clone, Debug)]
pub struct Channel { pub role: &'static str, pub notes: Vec<Note>, pub sound: u8, pub vol: u8, pub pan: u8, pub src: u8 }

#[derive(Clone, Debug)]
pub struct Piece { pub name: String, pub tempo_x10: u32, pub len_ticks: u32, pub channels: Vec<Channel> }

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError { TooBig(usize), Invalid(String) }

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::TooBig(n) => write!(f, "piece is {n} bytes; the FM-1 takes at most {MAX_BYTES}"),
            EncodeError::Invalid(s) => f.write_str(s),
        }
    }
}

/// Printable ASCII, at most 24 characters; accents folded, other characters dropped; never empty.
pub fn clean_name(s: &str) -> String {
    let flat: String = s.nfkd().filter(char::is_ascii).map(|c| if (' '..='~').contains(&c) { c } else { ' ' }).collect();
    let joined = flat.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let cut: String = joined.chars().take(NAME_LEN).collect();
    let cut = cut.trim_end();
    if cut.is_empty() { "untitled".into() } else { cut.into() }
}

fn validate(p: &Piece) -> Result<(), EncodeError> {
    let bad = |s: String| Err(EncodeError::Invalid(s));
    if p.channels.is_empty() || p.channels.len() > MAX_CH {
        return bad(format!("a piece has 1 to {MAX_CH} channels, not {}", p.channels.len()));
    }
    if !(300..=3000).contains(&p.tempo_x10) { return bad(format!("tempo must be 30.0 to 300.0 BPM, not {}", p.tempo_x10)); }
    if !(1..=MAX_TICKS).contains(&p.len_ticks) { return bad(format!("length must be 1 to {MAX_TICKS} ticks")); }
    if p.name != clean_name(&p.name) { return bad(format!("name must be printable ASCII, 1 to {NAME_LEN} characters: {:?}", p.name)); }
    for (i, c) in p.channels.iter().enumerate() {
        if !ROLES.contains(&c.role) || c.vol > 127 || c.pan > 127 || c.src > 15 {
            return bad(format!("channel {}: bad record", i + 1));
        }
        if c.notes.iter().any(|n| n.tick >= p.len_ticks || n.pitch > 127 || n.vel == 0 || n.vel > 127 || n.dur == 0) {
            return bad(format!("channel {}: note out of range", i + 1));
        }
    }
    Ok(())
}

fn events(c: &Channel) -> Vec<u8> {
    let mut notes = c.notes.clone();
    notes.sort();
    let mut out = Vec::with_capacity(notes.len() * 6);
    let mut t = 0u32;
    let mut ev = |d: u32, dur: u16, pitch: u8, vel: u8| {
        out.extend_from_slice(&(d as u16).to_le_bytes());
        out.extend_from_slice(&dur.to_le_bytes());
        out.push(pitch);
        out.push(vel);
    };
    for n in notes {
        let mut d = n.tick - t;
        while d > 65535 {
            ev(65535, 1, REST, 1);
            d -= 65535;
        }
        ev(d, n.dur, n.pitch, n.vel);
        t = n.tick;
    }
    out
}

pub fn crc32(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 { c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 }; }
    }
    !c
}

pub fn encode(p: &Piece) -> Result<Vec<u8>, EncodeError> {
    validate(p)?;
    let evs: Vec<Vec<u8>> = p.channels.iter().map(events).collect();
    let mut off = 48 + 16 * p.channels.len();
    let mut table = Vec::new();
    for (c, e) in p.channels.iter().zip(&evs) {
        let role = ROLES.iter().position(|r| *r == c.role).unwrap_or(10) as u8;
        table.extend_from_slice(&[role, c.sound, c.vol, c.pan, c.src, 0, 0, 0]);
        table.extend_from_slice(&((e.len() / 6) as u32).to_le_bytes());
        table.extend_from_slice(&(off as u32).to_le_bytes());
        off += e.len();
    }
    let total = off + 4;
    if total > MAX_BYTES { return Err(EncodeError::TooBig(total)); }
    let mut b = Vec::with_capacity(total);
    b.extend_from_slice(b"F1P1");
    b.extend_from_slice(&[1, p.channels.len() as u8, 0, 0]);
    b.extend_from_slice(&(p.tempo_x10 as u16).to_le_bytes());
    b.extend_from_slice(&(PPQ as u16).to_le_bytes());
    b.extend_from_slice(&p.len_ticks.to_le_bytes());
    let mut name = p.name.as_bytes().to_vec();
    name.resize(NAME_LEN, 0);
    b.extend_from_slice(&name);
    b.extend_from_slice(&(total as u32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&table);
    for e in &evs { b.extend_from_slice(e); }
    let crc = crc32(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    Ok(b)
}
