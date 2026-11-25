//! A MIDI file fitted to what COMPOSER plays, exactly as app/tools/fm1c/fit.py does it (same piece, same report).
use crate::f1p::{self, Channel, EncodeError, Note, Piece, Section, MAX_BYTES, MAX_CH, MAX_SECT, MAX_TICKS, PPQ};
use crate::smf;
use crate::sounds;

pub const MAX_INPUT: usize = 8 * 1024 * 1024;
const LOW: i64 = 24;
const HIGH: i64 = 96;
const BAR: u32 = PPQ * 4;
const PRIORITY: [&str; 11] = ["drums", "bass", "lead", "chords", "pad", "counter", "arp", "percussion", "drone", "fx", "other"];
const KEYWORDS: [(&str, &str); 15] = [("counter", "counter"), ("drum", "drums"), ("kit", "drums"), ("perc", "percussion"),
    ("bass", "bass"), ("chord", "chords"), ("comp", "chords"), ("pad", "pad"), ("string", "pad"), ("arp", "arp"),
    ("drone", "drone"), ("fx", "fx"), ("effect", "fx"), ("lead", "lead"), ("melody", "lead")];

pub struct Fitted { pub piece: Piece, pub bytes: Vec<u8>, pub report: Vec<String> }

impl Fitted {
    pub fn notes(&self) -> usize { self.piece.channels.iter().map(|c| c.notes.len()).sum() }
    /// fm1c convert's summary line for this piece written to `out`
    pub fn summary(&self, out: &str) -> String {
        let p = &self.piece;
        format!("{out}: '{}', {} channels, {} notes, {} bars, {} BPM, {} bytes, {} reductions", p.name, p.channels.len(),
                self.notes(), p.len_ticks / BAR, bpm(p.tempo_x10), self.bytes.len(), self.report.len())
    }
}

/// tempo x10 as Python's {x / 10:g}
pub fn bpm(x10: u32) -> String { if x10 % 10 == 0 { format!("{}", x10 / 10) } else { format!("{}.{}", x10 / 10, x10 % 10) } }

pub fn role_of(name: &str, src: u8, program: Option<u8>) -> &'static str {
    if src == 9 { return "drums"; }
    let n = name.to_lowercase();
    if let Some(first) = n.split_whitespace().next() && let Some(r) = f1p::ROLES.iter().find(|r| **r == first) {
        return r;
    }
    KEYWORDS.iter().find(|(k, _)| n.contains(k)).map(|(_, r)| *r)
        .or_else(|| sounds::role_hint(name, program)).unwrap_or("other")
}

fn floor_div(a: i128, b: i128) -> i128 { let q = a / b; if (a % b != 0) && ((a < 0) != (b < 0)) { q - 1 } else { q } }

/// Python's round() of a / b (half to even), a, b > 0
fn round_div(a: i128, b: i128) -> i128 {
    let (q, r) = (a / b, a % b);
    if 2 * r > b || (2 * r == b && q % 2 == 1) { q + 1 } else { q }
}

#[derive(Clone, Copy)]
struct N { tick: i128, pitch: u8, dur: i128, vel: u8 }

fn cap_poly(mut notes: Vec<N>, cap: usize) -> (Vec<N>, usize) {
    notes.sort_by_key(|n| (n.tick, -(n.vel as i32), n.pitch));
    let (mut out, mut sounding, mut dropped) = (Vec::new(), Vec::<i128>::new(), 0);
    for n in notes {
        sounding.retain(|&e| e > n.tick);
        if sounding.len() >= cap {
            dropped += 1;
            continue;
        }
        sounding.push(n.tick + n.dur);
        out.push(n);
    }
    (out, dropped)
}

/// `bytes` (a MIDI file) as a piece named `name`, at most `poly` notes at once per channel (drums 8)
pub fn fit(bytes: &[u8], name: &str, poly: usize) -> Result<Fitted, String> { fit_key(bytes, name, poly, None) }

/// fit, the piece carrying `key` (text2midi's brief: crate::brief_key) when it has sections (fit.py's key)
pub fn fit_key(bytes: &[u8], name: &str, poly: usize, key: Option<(u8, u8)>) -> Result<Fitted, String> {
    if bytes.len() > MAX_INPUT {
        return Err(format!("{} KB is larger than the 8 MB this tool reads", bytes.len() / 1024));
    }
    let smf::Parsed { mut parts, tempo_us, bends, tpq, mut markers } = smf::parse(bytes).map_err(|e| format!("not a readable MIDI file ({e})"))?;
    if parts.is_empty() { return Err("has no notes".into()); }
    let mut rep = Vec::new();
    if bends { rep.push("pitch bends ignored (notes play at the nearest key)".to_string()); }
    let tpq = if tpq == 0 { PPQ as i128 } else { tpq };
    for p in parts.iter_mut() { p.role = role_of(&p.name, p.src, p.program); }
    if parts.iter().any(|p| p.role == "drums") {
        let mut i = 0;
        while i < parts.len() {
            if parts[i].role == "percussion" {
                let p = parts.remove(i);
                if let Some(d) = parts.iter_mut().find(|q| q.role == "drums") { d.notes.extend(p.notes); }
                rep.push(format!("part '{}' merged into the drums", p.name));
            } else {
                i += 1;
            }
        }
    }
    parts.sort_by_key(|p| (PRIORITY.iter().position(|r| *r == p.role), -(p.notes.len() as i64)));
    for p in parts.iter().skip(MAX_CH) {
        let label = if p.name.is_empty() { p.role } else { p.name.as_str() };
        rep.push(format!("part '{label}' ({}, {} notes) dropped: the FM-1 plays {MAX_CH} channels", p.role, p.notes.len()));
    }
    parts.truncate(MAX_CH);

    let scale = |t: i128| floor_div(t * PPQ as i128 + floor_div(tpq, 2), tpq);
    let (mut chans, mut end, mut unknown, mut fm6) = (Vec::new(), 1i128, Vec::new(), 0);
    for p in &parts {
        let drums = p.role == "drums";
        let (mut notes, mut folded) = (Vec::new(), 0);
        for &(st, pitch, dur, vel) in &p.notes {
            let mut pitch = pitch as i64;
            if !drums && !(LOW..=HIGH).contains(&pitch) {
                folded += 1;
                while pitch < LOW { pitch += 12; }
                while pitch > HIGH { pitch -= 12; }
            }
            let s = scale(st);
            notes.push(N { tick: s, pitch: pitch as u8, dur: (scale(st + dur) - s).clamp(1, 65535), vel });
        }
        if folded > 0 { rep.push(format!("{}: {folded} notes moved by octaves into C1..C7", p.role)); }
        let cap = if drums { 8 } else { poly };
        let (notes, dropped) = cap_poly(notes, cap);
        if dropped > 0 { rep.push(format!("{}: {dropped} notes dropped (more than {cap} at once)", p.role)); }
        end = notes.iter().map(|n| n.tick + n.dur).fold(end, i128::max);
        let sound = match sounds::sound(&p.name, p.program, p.role) {
            None => {
                unknown.push(format!("'{}'", if p.name.is_empty() { p.role } else { p.name.as_str() }));
                255
            }
            Some(s) if s >= sounds::FM6_FIRST => {
                fm6 += 1;
                if fm6 > sounds::FM6_POOL {
                    let alt = sounds::FM6_ALT[(s - sounds::FM6_FIRST) as usize];
                    rep.push(format!("{}: {} plays as {} (the FM-1 plays at most {} FM6 sounds at once)", p.role,
                                     sounds::NAMES[s as usize], sounds::NAMES[alt as usize], sounds::FM6_POOL));
                    alt
                } else {
                    s
                }
            }
            Some(s) => s,
        };
        chans.push((p.role, notes, sound, p.vol.min(127), p.pan.min(127), p.src));
    }
    if !unknown.is_empty() {
        rep.push(format!("role default sound for {} (no known instrument or GM program)", unknown.join(", ")));
    }
    let length = (MAX_TICKS as i128).min(floor_div(end + BAR as i128 - 1, BAR as i128) * BAR as i128);
    let mut channels = Vec::new();
    for (role, notes, sound, vol, pan, src) in chans {
        let late = notes.iter().filter(|n| n.tick >= length).count();
        if late > 0 { rep.push(format!("{role}: {late} notes past {} bars dropped", MAX_TICKS / BAR)); }
        let mut kept = Vec::new();
        for n in notes.into_iter().filter(|n| n.tick < length) {
            if n.tick < 0 { return Err("a note lies before the start of the piece".into()); }
            kept.push(Note { tick: n.tick as u32, pitch: n.pitch, dur: n.dur as u16, vel: n.vel });
        }
        channels.push(Channel { role, notes: kept, sound, vol, pan, src, mode: f1p::AUTO, glide: 0 });   // (US-30: the sound's voice)
    }
    let exact = round_div(600_000_000, tempo_us);
    let tempo_x10 = exact.clamp(300, 3000) as u32;
    if tempo_x10 as i128 != exact {
        rep.push(format!("tempo {}.{} BPM clamped to {}", exact / 10, exact % 10, bpm(tempo_x10)));
    }
    // text2midi's form (a marker at each section's start) as the sections table, as fit.py's _sections
    markers.sort_by_key(|m| m.0);
    let (mut sections, mut same, mut late) = (Vec::<Section>::new(), 0, 0);
    for (t, text) in &markers {
        let s = scale(*t);
        if s >= length {
            late += 1;
        } else if sections.last().is_some_and(|l| l.tick as i128 == s) {
            same += 1;
        } else {
            sections.push(Section { tick: s as u32, name: f1p::section_name(text) });
        }
    }
    if same > 0 { rep.push(format!("{same} section markers dropped (at the tick of the one before)")); }
    if late > 0 { rep.push(format!("{late} section markers past the end dropped")); }
    if sections.len() > MAX_SECT {
        rep.push(format!("{} sections after the {MAX_SECT}th dropped (the FM-1 keeps {MAX_SECT})", sections.len() - MAX_SECT));
        sections.truncate(MAX_SECT);
    }
    let mut piece = Piece { name: f1p::clean_name(name), tempo_x10, len_ticks: length as u32, channels, sections, key };
    let bytes = loop {                          // too big: cut at a bar boundary until it fits
        match f1p::encode(&piece) {
            Ok(b) => break b,
            Err(EncodeError::TooBig(_)) if piece.len_ticks > BAR => {
                piece.len_ticks -= BAR;
                let len = piece.len_ticks;
                for c in piece.channels.iter_mut() { c.notes.retain(|n| n.tick < len); }
                piece.sections.retain(|s| s.tick < len);
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    if (piece.len_ticks as i128) < length {
        rep.push(format!("cut to {} bars to fit the FM-1's {} KB per piece", piece.len_ticks / BAR, MAX_BYTES / 1024));
    }
    if piece.channels.iter().all(|c| c.notes.is_empty()) {
        return Err(format!("no notes left to play after fitting it to the FM-1 ({})", rep.join("; ")));
    }
    Ok(Fitted { piece, bytes, report: rep })
}
