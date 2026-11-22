//! The Standard MIDI File format: writing a score's events, and reading notes back.

use crate::events::{BEND_SEMITONES, EventKind, TimedEvent};
use crate::export::MidiError;
use t2m_core::{Score, Tick, TimeFrame};

pub(crate) const PPQ: u16 = 960;

/// A format-1 file at 960 ticks per quarter: a conductor track, then one per part.
pub(crate) fn write_smf(score: &Score, events: &[TimedEvent]) -> Vec<u8> {
    let tracks = u16::try_from(score.tracks.len() + 1).unwrap_or(u16::MAX);
    let mut file = b"MThd".to_vec();
    file.extend_from_slice(&6u32.to_be_bytes());
    file.extend_from_slice(&1u16.to_be_bytes());
    file.extend_from_slice(&tracks.to_be_bytes());
    file.extend_from_slice(&PPQ.to_be_bytes());
    chunk(&mut file, &conductor(score));
    for (i, track) in score.tracks.iter().enumerate() {
        let mut bytes = Vec::new();
        let name = format!("{} \u{2013} {}", track.id, track.instrument);
        meta(&mut bytes, 0, 0x03, name.as_bytes());
        let mut at: Tick = 0;
        for event in events.iter().filter(|e| e.track == i) {
            for message in messages(&event.kind) {
                vlq(&mut bytes, u64::try_from(event.tick - at).unwrap_or(0));
                bytes.extend_from_slice(&message);
                at = event.tick;
            }
        }
        meta(&mut bytes, 0, 0x2F, &[]);
        chunk(&mut file, &bytes);
    }
    file
}

fn messages(kind: &EventKind) -> Vec<Vec<u8>> {
    match *kind {
        EventKind::NoteOn { channel, key, vel } => vec![vec![0x90 | channel, key, vel]],
        EventKind::NoteOff { channel, key } => vec![vec![0x80 | channel, key, 0]],
        EventKind::Bend { channel, value } => {
            vec![vec![
                0xE0 | channel,
                (value & 0x7F) as u8,
                (value >> 7) as u8,
            ]]
        }
        EventKind::Program { channel, program } => vec![vec![0xC0 | channel, program]],
        EventKind::BendRange { channel, semitones } => [
            (101, 0),
            (100, 0),
            (6, semitones.min(BEND_SEMITONES)),
            (38, 0),
            (101, 127),
            (100, 127),
        ]
        .iter()
        .map(|&(controller, value)| vec![0xB0 | channel, controller, value])
        .collect(),
    }
}

fn conductor(score: &Score) -> Vec<u8> {
    let mut bytes = Vec::new();
    meta(&mut bytes, 0, 0x03, b"conductor");
    let micros = 60_000_000u32 / u32::from(score.meta.tempo_bpm.max(1));
    meta(&mut bytes, 0, 0x51, &micros.to_be_bytes()[1..]);
    let (numerator, denominator) = match &score.meta.frame {
        TimeFrame::Meter(meter) => (meter.numerator, meter.denominator),
        TimeFrame::Cycle(cycle) => (cycle.beats, cycle.beat_unit),
    };
    let power = denominator.max(1).ilog2() as u8;
    meta(&mut bytes, 0, 0x58, &[numerator, power, 24, 8]);
    let mut at: Tick = 0;
    for section in &score.sections {
        let delta = u64::try_from(section.start - at).unwrap_or(0);
        meta(&mut bytes, delta, 0x06, section.id.as_bytes());
        at = section.start;
    }
    meta(&mut bytes, 0, 0x2F, &[]);
    bytes
}

fn meta(bytes: &mut Vec<u8>, delta: u64, kind: u8, data: &[u8]) {
    vlq(bytes, delta);
    bytes.extend_from_slice(&[0xFF, kind]);
    vlq(bytes, data.len() as u64);
    bytes.extend_from_slice(data);
}

fn chunk(file: &mut Vec<u8>, body: &[u8]) {
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
    file.extend_from_slice(body);
}

/// A variable-length quantity: seven bits at a time, the last byte without the high bit.
fn vlq(bytes: &mut Vec<u8>, mut value: u64) {
    let mut groups = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        groups.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    groups.reverse();
    bytes.extend_from_slice(&groups);
}

/// A note read back from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadNote {
    pub start: Tick,
    pub end: Tick,
    pub key: u8,
    pub vel: u8,
    pub channel: u8,
}

/// What one track of a file holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadTrack {
    pub name: String,
    pub notes: Vec<ReadNote>,
    /// (tick, channel, 14-bit value).
    pub bends: Vec<(Tick, u8, u16)>,
    /// (channel, program).
    pub programs: Vec<(u8, u8)>,
    /// (channel, semitones) set through registered parameter 0.
    pub bend_ranges: Vec<(u8, u8)>,
    pub markers: Vec<(Tick, String)>,
    /// Microseconds per quarter note, and the time signature as written.
    pub tempo_micros: Option<u32>,
    pub time_signature: Option<(u8, u8)>,
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, MidiError> {
        let byte = *self
            .data
            .get(self.at)
            .ok_or_else(|| bad("file ends early"))?;
        self.at += 1;
        Ok(byte)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], MidiError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.data.len());
        let end = end.ok_or_else(|| bad("file ends early"))?;
        let slice = &self.data[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn vlq(&mut self) -> Result<u64, MidiError> {
        let mut value = 0u64;
        for _ in 0..5 {
            let byte = self.byte()?;
            value = (value << 7) | u64::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(bad("a length is too long"))
    }
}

fn bad(reason: &str) -> MidiError {
    MidiError::Parse {
        reason: reason.to_string(),
    }
}

/// Reads every track of a format-0/1 file. Notes still sounding at the end are an error.
pub fn read_notes(bytes: &[u8]) -> Result<Vec<ReadTrack>, MidiError> {
    let mut r = Reader { data: bytes, at: 0 };
    if r.take(4)? != b"MThd" {
        return Err(bad("not a MIDI file"));
    }
    let header = r.take(4)?.to_vec();
    if header != [0, 0, 0, 6] {
        return Err(bad("unexpected header length"));
    }
    let head = r.take(6)?.to_vec();
    let count = u16::from_be_bytes([head[2], head[3]]);
    let mut tracks = Vec::new();
    for _ in 0..count {
        if r.take(4)? != b"MTrk" {
            return Err(bad("a track is missing"));
        }
        let len = u32::from_be_bytes(r.take(4)?.try_into().map_err(|_| bad("bad length"))?);
        let end = r.at + len as usize;
        if end > bytes.len() {
            return Err(bad("a track runs past the end of the file"));
        }
        tracks.push(read_track(&mut r, end)?);
    }
    Ok(tracks)
}

fn read_track(r: &mut Reader, end: usize) -> Result<ReadTrack, MidiError> {
    let mut track = ReadTrack::default();
    let mut tick: Tick = 0;
    let mut status = 0u8;
    let mut open: Vec<(u8, u8, Tick, u8)> = Vec::new();
    // Registered-parameter state per channel: (101, 100) selects, 6 sets.
    let mut rpn = [(0u8, 0u8); 16];
    while r.at < end {
        tick += i64::try_from(r.vlq()?).map_err(|_| bad("time too large"))?;
        let first = r.byte()?;
        if first == 0xFF {
            let kind = r.byte()?;
            let n = r.vlq()? as usize;
            let body = r.take(n)?.to_vec();
            match (kind, body.as_slice()) {
                (0x03, _) => track.name = String::from_utf8_lossy(&body).into_owned(),
                (0x06, _) => track
                    .markers
                    .push((tick, String::from_utf8_lossy(&body).into_owned())),
                (0x51, [a, b, c]) => {
                    track.tempo_micros = Some(u32::from_be_bytes([0, *a, *b, *c]));
                }
                (0x58, [n, p, ..]) => {
                    track.time_signature = Some((*n, 1u8.checked_shl(u32::from(*p)).unwrap_or(0)));
                }
                _ => {}
            }
            continue;
        }
        if first >= 0x80 {
            status = first;
        } else {
            r.at -= 1; // running status: the byte is data
        }
        let channel = status & 0x0F;
        match status & 0xF0 {
            0x90 | 0x80 => {
                let (key, vel) = (r.byte()?, r.byte()?);
                if status & 0xF0 == 0x90 && vel > 0 {
                    open.push((channel, key, tick, vel));
                } else {
                    let i = open
                        .iter()
                        .position(|o| o.0 == channel && o.1 == key)
                        .ok_or_else(|| bad("a note ends that never began"))?;
                    let (channel, key, start, vel) = open.remove(i);
                    track.notes.push(ReadNote {
                        start,
                        end: tick,
                        key,
                        vel,
                        channel,
                    });
                }
            }
            0xE0 => {
                let (lsb, msb) = (r.byte()?, r.byte()?);
                let value = u16::from(lsb) | (u16::from(msb) << 7);
                track.bends.push((tick, channel, value));
            }
            0xC0 => track.programs.push((channel, r.byte()?)),
            0xB0 => {
                let (controller, value) = (r.byte()?, r.byte()?);
                let state = &mut rpn[usize::from(channel)];
                match controller {
                    101 => state.0 = value,
                    100 => state.1 = value,
                    6 if *state == (0, 0) => track.bend_ranges.push((channel, value)),
                    _ => {}
                }
            }
            0xA0 => {
                r.take(2)?;
            }
            0xD0 => {
                r.take(1)?;
            }
            _ => return Err(bad("unsupported message")),
        }
    }
    if !open.is_empty() {
        return Err(bad("a note is still sounding at the end of a track"));
    }
    track.notes.sort_by_key(|n| (n.start, n.key));
    Ok(track)
}

#[cfg(test)]
#[path = "smf_tests.rs"]
mod tests;
