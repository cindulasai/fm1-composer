//! A score as timed MIDI events, with a choice of how microtones are carried.

use t2m_core::brief::Role;
use t2m_core::{Knowledge, Note, Score, Tick, Tuning};

/// The pitch-bend range, in semitones, set on every channel that carries microtones.
pub(crate) const BEND_SEMITONES: u8 = 48;
const CENTRE: f64 = 8192.0;
const DRUM_CHANNEL: u8 = 9;
/// The channels a pitched part may use, in order (channel 10 is the kit).
const PITCHED: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
/// Below this many cents a pitch counts as sitting exactly on its key.
const ON_KEY: f64 = 0.01;

/// How microtonal pitches reach the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningProfile {
    /// Each microtonal note gets its own channel and a pitch bend (range ±48 semitones).
    Mpe,
    /// Notes become keys of a retuned keyboard; Scala files describe the scale.
    TuningFile,
    /// Every note moves to the nearest 12-tone key; bends are dropped.
    Approx12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    NoteOn { channel: u8, key: u8, vel: u8 },
    NoteOff { channel: u8, key: u8 },
    Bend { channel: u8, value: u16 },
    Program { channel: u8, program: u8 },
    BendRange { channel: u8, semitones: u8 },
}

impl EventKind {
    /// Order at one tick: notes end, setup, bends, then notes begin.
    fn order(&self) -> u8 {
        match self {
            EventKind::NoteOff { .. } => 0,
            EventKind::Program { .. } | EventKind::BendRange { .. } => 1,
            EventKind::Bend { .. } => 2,
            EventKind::NoteOn { .. } => 3,
        }
    }
}

/// One event, on the track at `track` in the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    pub tick: Tick,
    pub track: usize,
    pub kind: EventKind,
}

/// Something the file could not say exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiWarning {
    /// More simultaneous microtonal notes than channels: the oldest note was cut short.
    VoiceLimit { track: String, notes: usize },
    /// Notes moved to the nearest key of the target (12-tone, or the tuning's degrees).
    Approximated { notes: usize },
    /// Notes whose bend curve could not be carried and was dropped.
    BendsDropped { notes: usize },
    /// Notes whose key fell outside 0–127 and was clamped.
    Clamped { notes: usize },
}

/// Events sorted by tick, plus what could not be said exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventStream {
    pub events: Vec<TimedEvent>,
    pub warnings: Vec<MidiWarning>,
}

/// The score's events with profile `Mpe` (the one playback shares).
pub fn to_events(score: &Score, kb: &Knowledge) -> EventStream {
    let tuning = kb.tuning(&score.meta.tuning_id);
    to_events_with(score, kb, TuningProfile::Mpe, tuning)
}

/// The score's events under `profile`. `tuning` matters only for `TuningFile`;
/// without one the score is treated as 12-tone.
pub fn to_events_with(
    score: &Score,
    kb: &Knowledge,
    profile: TuningProfile,
    tuning: Option<&Tuning>,
) -> EventStream {
    let (tonic_key, tonic_off) = score.meta.tonic.midi_and_offset(0);
    let pitcher = Pitcher {
        tonic_cents: f64::from(tonic_key) * 100.0 + f64::from(tonic_off),
        tonic_key: i64::from(tonic_key),
        tuning: tuning.filter(|t| profile == TuningProfile::TuningFile && !is_twelve_tone(t)),
    };
    let mut out = Build::default();
    let pitched: Vec<usize> = (0..score.tracks.len())
        .filter(|&i| !is_kit(score.tracks[i].role))
        .collect();
    let base = |i: usize| -> u8 {
        let ordinal = pitched.iter().position(|&p| p == i).unwrap_or(0);
        PITCHED[ordinal.min(PITCHED.len() - 1)]
    };
    let micro: Vec<usize> = pitched
        .iter()
        .copied()
        .filter(|&i| {
            profile == TuningProfile::Mpe
                && score.tracks[i]
                    .notes
                    .iter()
                    .any(|n| n.bend.is_some() || pitcher.offset(n).abs() > ON_KEY)
        })
        .collect();
    // Channels not taken by a part's own channel are shared out among microtonal parts.
    let spare: Vec<u8> = PITCHED.iter().copied().skip(pitched.len()).collect();
    for (i, track) in score.tracks.iter().enumerate() {
        let program = kb.instrument(&track.instrument).map_or(0, |x| x.gm_program);
        if is_kit(track.role) {
            out.notes_plain(i, &track.notes, DRUM_CHANNEL, kit_key);
            continue;
        }
        let own = base(i);
        if let Some(slot) = micro.iter().position(|&m| m == i) {
            let mut pool = vec![own];
            pool.extend(spare.iter().copied().skip(slot).step_by(micro.len()));
            for &channel in &pool {
                out.setup(i, channel, program, true);
            }
            out.notes_micro(i, &track.id, &track.notes, &pool, &pitcher);
        } else {
            out.setup(i, own, program, false);
            out.notes_plain(i, &track.notes, own, |n| pitcher.key(n));
            out.dropped_bends(&track.notes);
            if profile != TuningProfile::Mpe {
                out.moved += track.notes.iter().filter(|n| pitcher.moved(n)).count();
            }
        }
    }
    out.finish()
}

/// Where a note stops: a note always lasts at least one tick, so its off never precedes its on.
fn end_of(note: &Note) -> Tick {
    note.start.saturating_add(note.dur.max(1))
}

fn is_kit(role: Role) -> bool {
    matches!(role, Role::Drums | Role::Percussion)
}

fn is_twelve_tone(tuning: &Tuning) -> bool {
    tuning.period == 1200
        && tuning.cents.len() == 12
        && tuning
            .cents
            .iter()
            .enumerate()
            .all(|(i, &c)| c == i as i32 * 100)
}

fn kit_key(note: &Note) -> (u8, bool) {
    clamp_key((f64::from(note.pitch.cents) / 100.0).round() as i64)
}

fn clamp_key(key: i64) -> (u8, bool) {
    let clamped = key.clamp(0, 127);
    (u8::try_from(clamped).unwrap_or(0), clamped != key)
}

/// How a pitch becomes a key (and, for microtones, an offset from it).
struct Pitcher<'a> {
    tonic_cents: f64,
    tonic_key: i64,
    tuning: Option<&'a Tuning>,
}

impl Pitcher<'_> {
    fn total(&self, note: &Note) -> f64 {
        self.tonic_cents + f64::from(note.pitch.cents)
    }

    /// The nearest key, a quarter-tone exactly between two rounding up.
    fn nearest(&self, note: &Note) -> i64 {
        ((self.total(note) + 50.0) / 100.0).floor() as i64
    }

    /// The nearest key for a bent note: a tie rounds down, so the bend is +50 and never -50.
    fn nearest_bent(&self, note: &Note) -> i64 {
        (self.total(note) / 100.0 - 0.5).ceil() as i64
    }

    /// Cents from the bent note's key to its pitch, in -49.99..=50.
    fn offset(&self, note: &Note) -> f64 {
        self.total(note) - self.nearest_bent(note) as f64 * 100.0
    }

    /// The key the note sounds on in a plain file; under a tuning, its nearest degree.
    fn key(&self, note: &Note) -> (u8, bool) {
        let Some(tuning) = self.tuning else {
            return clamp_key(self.nearest(note));
        };
        let degrees = tuning.cents.len() as i64;
        let period = i64::from(tuning.period.max(1));
        let cents = i64::from(note.pitch.cents);
        let (octave, within) = (cents.div_euclid(period), cents.rem_euclid(period));
        let closest = tuning
            .cents
            .iter()
            .map(|&c| i64::from(c))
            .chain(std::iter::once(period))
            .enumerate()
            .min_by_key(|&(_, c)| (within - c).abs())
            .map_or(0, |(d, _)| d as i64);
        clamp_key(self.tonic_key + octave * degrees + closest)
    }

    /// Whether the note's pitch is not exactly a degree of the tuning (or key).
    fn moved(&self, note: &Note) -> bool {
        match self.tuning {
            None => (self.total(note) - self.nearest(note) as f64 * 100.0).abs() > ON_KEY,
            Some(tuning) => {
                let period = i64::from(tuning.period.max(1));
                let within = i64::from(note.pitch.cents).rem_euclid(period);
                within != 0 && !tuning.cents.iter().any(|&c| i64::from(c) == within)
            }
        }
    }
}

/// The 14-bit bend value for `cents` from the written key.
pub(crate) fn bend_value(cents: f64) -> u16 {
    let range = f64::from(BEND_SEMITONES) * 100.0;
    let value = (CENTRE + (cents / range * CENTRE).round()).clamp(0.0, 16383.0);
    value as u16
}

#[derive(Default)]
struct Build {
    events: Vec<TimedEvent>,
    voice_limit: Vec<(String, usize)>,
    moved: usize,
    dropped: usize,
    clamped: usize,
    /// Indices of events to remove before sorting.
    dead: Vec<usize>,
}

/// A channel in a microtonal part's pool: when it is free, and its sounding note.
struct Voice {
    channel: u8,
    busy_until: Tick,
    off_at: Option<usize>,
    /// The bend events of the sounding note's curve, which die with it if it is cut.
    curve_at: Vec<usize>,
}

impl Build {
    fn push(&mut self, tick: Tick, track: usize, kind: EventKind) -> usize {
        self.events.push(TimedEvent { tick, track, kind });
        self.events.len() - 1
    }

    fn setup(&mut self, track: usize, channel: u8, program: u8, bends: bool) {
        self.push(0, track, EventKind::Program { channel, program });
        if bends {
            let semitones = BEND_SEMITONES;
            self.push(0, track, EventKind::BendRange { channel, semitones });
        }
    }

    fn notes_plain(
        &mut self,
        track: usize,
        notes: &[Note],
        channel: u8,
        key_of: impl Fn(&Note) -> (u8, bool),
    ) {
        for note in notes {
            let (key, clamped) = key_of(note);
            self.clamped += usize::from(clamped);
            let vel = note.vel.clamp(1, 127);
            self.push(note.start, track, EventKind::NoteOn { channel, key, vel });
            self.push(end_of(note), track, EventKind::NoteOff { channel, key });
        }
    }

    fn dropped_bends(&mut self, notes: &[Note]) {
        self.dropped += notes.iter().filter(|n| n.bend.is_some()).count();
    }

    fn notes_micro(
        &mut self,
        track: usize,
        id: &str,
        notes: &[Note],
        pool: &[u8],
        pitcher: &Pitcher,
    ) {
        let mut voices: Vec<Voice> = pool
            .iter()
            .map(|&channel| Voice {
                channel,
                busy_until: Tick::MIN,
                off_at: None,
                curve_at: Vec::new(),
            })
            .collect();
        let mut cut = 0;
        for note in notes {
            let free = voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.busy_until <= note.start)
                .min_by_key(|(_, v)| v.busy_until)
                .map(|(i, _)| i);
            let slot = free.unwrap_or_else(|| {
                cut += 1;
                voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.busy_until)
                    .map_or(0, |(i, _)| i)
            });
            if free.is_none()
                && let Some(at) = voices[slot].off_at
            {
                // The oldest note stops where the new one begins.
                self.events[at].tick = note.start;
                // Its remaining curve points must not bend the note that takes the channel.
                for &i in &voices[slot].curve_at {
                    if self.events[i].tick >= note.start {
                        self.dead.push(i);
                    }
                }
            }
            let channel = voices[slot].channel;
            let (key, clamped) = clamp_key(pitcher.nearest_bent(note));
            self.clamped += usize::from(clamped);
            let offset = pitcher.offset(note);
            let value = bend_value(offset);
            self.push(note.start, track, EventKind::Bend { channel, value });
            let vel = note.vel.clamp(1, 127);
            self.push(note.start, track, EventKind::NoteOn { channel, key, vel });
            let mut curve_at = Vec::new();
            if let Some(curve) = &note.bend {
                for &(after, cents) in &curve.0 {
                    let at = note
                        .start
                        .saturating_add(after.clamp(0, (note.dur - 1).max(0)));
                    let value = bend_value(offset + f64::from(cents));
                    curve_at.push(self.push(at, track, EventKind::Bend { channel, value }));
                }
            }
            let end = end_of(note);
            let off = self.push(end, track, EventKind::NoteOff { channel, key });
            voices[slot].busy_until = end;
            voices[slot].off_at = Some(off);
            voices[slot].curve_at = curve_at;
        }
        if cut > 0 {
            self.voice_limit.push((id.to_string(), cut));
        }
    }

    fn finish(mut self) -> EventStream {
        let dead = std::mem::take(&mut self.dead);
        let mut index = 0;
        self.events.retain(|_| {
            index += 1;
            !dead.contains(&(index - 1))
        });
        self.events.sort_by_key(|e| (e.tick, e.kind.order()));
        let mut warnings = Vec::new();
        for (track, notes) in self.voice_limit {
            warnings.push(MidiWarning::VoiceLimit { track, notes });
        }
        if self.moved > 0 {
            warnings.push(MidiWarning::Approximated { notes: self.moved });
        }
        if self.dropped > 0 {
            warnings.push(MidiWarning::BendsDropped {
                notes: self.dropped,
            });
        }
        if self.clamped > 0 {
            warnings.push(MidiWarning::Clamped {
                notes: self.clamped,
            });
        }
        EventStream {
            events: self.events,
            warnings,
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
