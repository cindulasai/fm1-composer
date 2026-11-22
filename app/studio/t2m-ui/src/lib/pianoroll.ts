import type { NoteTuple } from "./types";

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** Cents within which a note counts as sitting exactly on a semitone. */
export const IN_TUNE_CENTS = 5;

/** The semitone a pitch is nearest to, and how many cents it is above (+) or below (-) it. */
export function nearestSemitone(pitch: number): { semitone: number; cents: number } {
  const semitone = Math.round(pitch);
  return { semitone, cents: Math.round((pitch - semitone) * 100) };
}

/** "C4", "F#3"; with a cents suffix such as "A4 +50¢" when the note is off the semitone. */
export function pitchLabel(pitch: number): string {
  if (!Number.isFinite(pitch)) return "?";
  const { semitone, cents } = nearestSemitone(pitch);
  const octave = Math.floor(semitone / 12) - 1;
  const name = NAMES[((semitone % 12) + 12) % 12] + octave;
  if (Math.abs(cents) <= IN_TUNE_CENTS) return name;
  return `${name} ${cents > 0 ? "+" : ""}${cents}¢`;
}

export interface Roll {
  low: number;
  high: number;
  rows: number;
}

/** The semitone rows shown: from just below the lowest note to just above the highest, at least an octave. */
/** Pitches outside this are drawn off-screen and ignored when sizing the roll. */
export const PITCH_MIN = -24;
export const PITCH_MAX = 160;

function usable(n: NoteTuple): boolean {
  return Number.isFinite(n[2]) && n[2] >= PITCH_MIN && n[2] <= PITCH_MAX;
}

/** Lowest and highest usable pitch, or null when there is none (a loop, so any number of notes is fine). */
function extent(notes: NoteTuple[]): [number, number] | null {
  let lo = Infinity;
  let hi = -Infinity;
  for (const n of notes) {
    if (!usable(n)) continue;
    if (n[2] < lo) lo = n[2];
    if (n[2] > hi) hi = n[2];
  }
  return lo === Infinity ? null : [lo, hi];
}

/** The semitone rows shown: from just below the lowest note to just above the highest, at least an octave. */
export function rollRange(notes: NoteTuple[]): Roll {
  const found = extent(notes);
  if (!found) return { low: 60, high: 72, rows: 13 };
  let low = Math.floor(found[0]) - 1;
  let high = Math.ceil(found[1]) + 1;
  if (high - low < 12) {
    const extra = 12 - (high - low);
    low -= Math.floor(extra / 2);
    high += Math.ceil(extra / 2);
  }
  return { low, high, rows: high - low + 1 };
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const ROW_HEIGHT = 12;
export const KEY_WIDTH = 44;

/** The rectangle of a note: x by tick, y by pitch (so a +50 cent note sits between two rows). */
export function noteBox(note: NoteTuple, end: number, width: number, roll: Roll): Box {
  const area = Math.max(1, width - KEY_WIDTH);
  const span = Math.max(1, end);
  const x = KEY_WIDTH + (Math.max(0, Math.min(note[0], span)) / span) * area;
  const x2 = KEY_WIDTH + (Math.max(0, Math.min(note[0] + note[1], span)) / span) * area;
  const y = (roll.high - note[2]) * ROW_HEIGHT;
  return { x, y, w: Math.max(2, x2 - x), h: ROW_HEIGHT - 2 };
}

export function rollHeight(roll: Roll): number {
  return roll.rows * ROW_HEIGHT;
}

/** Index of the note under (x, y), the later one when several overlap, or -1. */
export function noteAt(x: number, y: number, notes: NoteTuple[], end: number, width: number, roll: Roll): number {
  for (let i = notes.length - 1; i >= 0; i--) {
    const b = noteBox(notes[i], end, width, roll);
    if (x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h) return i;
  }
  return -1;
}

/** Small rectangles for a thumbnail of a take, `w` by `h` pixels; empty when there is nothing to draw. */
export function thumbBoxes(notes: NoteTuple[], end: number, w: number, h: number): Box[] {
  const found = extent(notes);
  if (!found || end <= 0 || w <= 0 || h <= 0) return [];
  const lo = found[0];
  const span = Math.max(12, found[1] - lo);
  const boxes: Box[] = [];
  for (const n of notes) {
    if (!usable(n) || !Number.isFinite(n[0]) || !Number.isFinite(n[1])) continue;
    const x = (Math.max(0, Math.min(n[0], end)) / end) * w;
    const x2 = (Math.max(0, Math.min(n[0] + n[1], end)) / end) * w;
    boxes.push({ x, y: (1 - (n[2] - lo) / span) * (h - 2), w: Math.max(1, x2 - x), h: 2 });
  }
  return boxes;
}
