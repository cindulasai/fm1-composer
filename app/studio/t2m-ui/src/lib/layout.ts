import type { NoteTuple, SectionView } from "./types";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const RIBBON_HEIGHT = 26;
export const LANE_HEIGHT = 64;
export const HEADER_WIDTH = 168;

/** Horizontal position of a tick on a canvas whose time area starts at `left`. */
export function tickToX(tick: number, end: number, width: number, left = HEADER_WIDTH): number {
  if (end <= 0) return left;
  return left + (Math.max(0, Math.min(tick, end)) / end) * Math.max(0, width - left);
}

/** The lowest and highest pitch of a lane, padded so notes never touch its edges. */
export function pitchRange(notes: NoteTuple[]): [number, number] {
  if (notes.length === 0) return [60, 72];
  let lo = Infinity;
  let hi = -Infinity;
  for (const n of notes) {
    lo = Math.min(lo, n[2]);
    hi = Math.max(hi, n[2]);
  }
  if (hi - lo < 12) {
    const mid = (hi + lo) / 2;
    lo = mid - 6;
    hi = mid + 6;
  }
  return [lo - 1, hi + 1];
}

/** The rectangle of one note inside a lane starting at `laneTop`. */
export function noteRect(
  note: NoteTuple,
  end: number,
  width: number,
  laneTop: number,
  range: [number, number],
  laneHeight = LANE_HEIGHT,
): Rect {
  const x = tickToX(note[0], end, width);
  const x2 = tickToX(note[0] + note[1], end, width);
  const span = Math.max(1e-6, range[1] - range[0]);
  const inner = laneHeight - 8;
  const y = laneTop + 4 + (1 - (note[2] - range[0]) / span) * inner;
  return { x, y: y - 1.5, w: Math.max(1.5, x2 - x), h: 3 };
}

/** Which section the canvas x position falls in, or null (header area, past the end). */
export function sectionAt(x: number, sections: SectionView[], end: number, width: number): string | null {
  if (x < HEADER_WIDTH || end <= 0) return null;
  for (const s of sections) {
    if (x >= tickToX(s.start, end, width) && x < tickToX(s.end, end, width)) return s.id;
  }
  return null;
}

/** Which track lane the canvas y position falls in, or -1. */
export function laneAt(y: number, count: number): number {
  if (y < RIBBON_HEIGHT) return -1;
  const i = Math.floor((y - RIBBON_HEIGHT) / LANE_HEIGHT);
  return i >= 0 && i < count ? i : -1;
}

export function canvasHeight(trackCount: number): number {
  return RIBBON_HEIGHT + trackCount * LANE_HEIGHT;
}
