//! A Standard MIDI File read as fm1c reads it (mido 1.3, clip=True, then fit.py's _parts): the same files refused,
//! the same notes per (track, channel) in the same order.
use std::collections::{HashMap, VecDeque};

const MAX_MESSAGE: i128 = 1_000_000;             // mido's MAX_MESSAGE_LENGTH
const SAT: i128 = 1 << 100;                      // (Python's ints do not overflow; ticks this far are past the end anyway)

pub struct Part {
    pub name: String,
    pub src: u8,
    /// tick, pitch, dur, vel (source ticks)
    pub notes: Vec<(i128, u8, i128, u8)>,
    pub vol: u8,
    pub pan: u8,
    pub role: &'static str,
}

pub struct Parsed { pub parts: Vec<Part>, pub tempo_us: i128, pub bends: bool, pub tpq: i128 }

struct R<'a> { d: &'a [u8], at: usize }

impl<'a> R<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.d.get(self.at).ok_or("the file ends in the middle of a message")?;
        self.at += 1;
        Ok(b)
    }
    fn take(&mut self, n: i128) -> Result<&'a [u8], String> {
        if n > MAX_MESSAGE { return Err(format!("a message of {n} bytes is longer than {MAX_MESSAGE}")); }
        let n = n.max(0) as usize;
        let s = self.d.get(self.at..self.at + n).ok_or("the file ends in the middle of a message")?;
        self.at += n;
        Ok(s)
    }
    fn varint(&mut self) -> Result<i128, String> {
        let mut v: i128 = 0;
        loop {
            let b = self.byte()?;
            v = ((v << 7) | (b & 0x7F) as i128).min(SAT);
            if b < 0x80 { return Ok(v); }
        }
    }
    fn chunk(&mut self) -> Result<(&'a [u8], usize), String> {
        let h = self.d.get(self.at..self.at + 8).ok_or("the file ends early")?;
        self.at += 8;
        Ok((&h[..4], u32::from_be_bytes([h[4], h[5], h[6], h[7]]) as usize))
    }
}

/// mido decodes meta text as Latin-1; fit.py then reads it as UTF-8 when it is
fn text(d: &[u8]) -> String {
    match std::str::from_utf8(d) { Ok(s) => s.to_string(), Err(_) => d.iter().map(|&b| b as char).collect() }
}

/// the meta events mido refuses to decode
fn meta_ok(t: u8, d: &[u8]) -> Result<(), String> {
    let short = |n: usize| if d.len() < n { Err(format!("meta event 0x{t:02x} is too short")) } else { Ok(()) };
    match t {
        0x00 => if d.len() == 1 { short(2) } else { Ok(()) },
        0x20 => short(1),
        0x51 => short(3),
        0x54 => {
            short(5)?;
            if d[0] >> 5 > 3 || d[1] > 59 || d[2] > 59 || d[4] > 99 { Err("bad SMPTE offset".into()) } else { Ok(()) }
        }
        0x58 => short(4),
        0x59 => {
            short(2)?;
            if !(-7..=7).contains(&(d[0] as i8)) || d[1] > 1 { Err("bad key signature".into()) } else { Ok(()) }
        }
        _ => Ok(()),
    }
}

fn length(status: u8) -> Result<usize, String> {
    Ok(match status {
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 3,
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        0xF6 | 0xF8 | 0xFA | 0xFB | 0xFC | 0xFE => 1,
        _ => return Err(format!("undefined status byte 0x{status:02x}")),
    })
}

pub fn parse(d: &[u8]) -> Result<Parsed, String> {
    let mut r = R { d, at: 0 };
    let (name, size) = r.chunk().map_err(|_| "not a MIDI file (too short)".to_string())?;
    if name != b"MThd" { return Err("not a MIDI file (no MThd)".into()); }
    let avail = size.min(d.len() - r.at);
    let h = &d[r.at..r.at + avail];
    r.at += avail;
    if avail < 6 { return Err("the MIDI header is cut short".into()); }
    let ntracks = i16::from_be_bytes([h[2], h[3]]);
    let tpq = i16::from_be_bytes([h[4], h[5]]) as i128;
    let mut parts: Vec<Part> = Vec::new();
    let mut part_ix: HashMap<(usize, u8), usize> = HashMap::new();
    let (mut tempo, mut bends) = (None::<i128>, false);
    for ti in 0..ntracks.max(0) as usize {
        let (cname, size) = r.chunk()?;
        if cname != b"MTrk" { return Err("no MTrk header at start of track".into()); }
        let start = r.at;
        let (mut name, mut t, mut last) = (String::new(), 0i128, None::<u8>);
        let mut held: Vec<((u8, u8), VecDeque<(i128, u8)>)> = Vec::new();
        let mut held_ix: HashMap<(u8, u8), usize> = HashMap::new();
        let mut cc = [[None::<u8>; 2]; 16];
        let mut add = |parts: &mut Vec<Part>, ch: u8, name: &str, note: (i128, u8, i128, u8)| {
            let i = *part_ix.entry((ti, ch)).or_insert_with(|| {
                parts.push(Part { name: name.to_string(), src: ch, notes: Vec::new(), vol: 100, pan: 64, role: "other" });
                parts.len() - 1
            });
            parts[i].notes.push(note);
        };
        while r.at - start != size {
            if r.at - start > size { return Err("a track runs past its end".into()); }
            t = (t + r.varint()?).min(SAT);
            let mut status = r.byte()?;
            let mut peek = None;
            if status < 0x80 {
                peek = Some(status);
                status = last.ok_or("running status without last_status")?;
            } else if status != 0xFF {
                last = Some(status);
            }
            if status == 0xFF {
                let mt = r.byte()?;
                let n = r.varint()?;
                let md = r.take(n)?;
                meta_ok(mt, md)?;
                if mt == 0x03 && name.is_empty() { name = text(md); }
                if mt == 0x51 && tempo.is_none() { tempo = Some(((md[0] as i128) << 16) | ((md[1] as i128) << 8) | md[2] as i128); }
                continue;
            }
            if status == 0xF0 || status == 0xF7 {
                let n = r.varint()?;
                r.take(n)?;
                continue;
            }
            let len = length(status)?;
            let mut data: Vec<u8> = Vec::with_capacity(2);
            if let Some(p) = peek {
                if len < 2 { return Err("running status on a one-byte message".into()); }
                data.push(p);
                data.extend_from_slice(r.take(len as i128 - 2)?);
            } else {
                data.extend_from_slice(r.take(len as i128 - 1)?);
            }
            for b in data.iter_mut() { *b = (*b).min(127); }
            if status >= 0xF0 { continue; }
            let ch = status & 15;
            match status & 0xF0 {
                0x90 if data[1] > 0 => {
                    let k = (ch, data[0]);
                    let i = *held_ix.entry(k).or_insert_with(|| { held.push((k, VecDeque::new())); held.len() - 1 });
                    held[i].1.push_back((t, data[1]));
                }
                0x80 | 0x90 => {
                    if let Some(&i) = held_ix.get(&(ch, data[0])) && let Some((st, v)) = held[i].1.pop_front() {
                        add(&mut parts, ch, &name, (st, data[0], (t - st).max(1), v));
                    }
                }
                0xB0 if data[0] == 7 || data[0] == 10 => {
                    let slot = &mut cc[ch as usize][(data[0] == 10) as usize];
                    if slot.is_none() { *slot = Some(data[1]); }
                }
                0xE0 if (data[0] as u16 | (data[1] as u16) << 7) != 8192 => bends = true,
                _ => {}
            }
        }
        for ((ch, note), q) in &held {                // never released: end at the track's end
            for &(st, v) in q { add(&mut parts, *ch, &name, (st, *note, (t - st).max(1), v)); }
        }
        for (&(tti, ch), &i) in part_ix.iter() {
            if tti == ti {
                let p = &mut parts[i];
                if p.name.is_empty() { p.name = name.clone(); }
                p.vol = cc[ch as usize][0].unwrap_or(100);
                p.pan = cc[ch as usize][1].unwrap_or(64);
            }
        }
    }
    let tempo_us = match tempo { Some(0) | None => 500_000, Some(v) => v };
    Ok(Parsed { parts, tempo_us, bends, tpq })
}
