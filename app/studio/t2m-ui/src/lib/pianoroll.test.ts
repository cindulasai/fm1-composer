import { describe, expect, it } from "vitest";
import { KEY_WIDTH, ROW_HEIGHT, noteAt, noteBox, pitchLabel, rollHeight, rollRange } from "./pianoroll";
import type { NoteTuple } from "./types";

describe("piano roll", () => {
  it("44.1 names pitches and shows cents only when a note is off its semitone", () => {
    expect(pitchLabel(60)).toBe("C4");
    expect(pitchLabel(69)).toBe("A4");
    expect(pitchLabel(61)).toBe("C#4");
    expect(pitchLabel(0)).toBe("C-1");
    expect(pitchLabel(127)).toBe("G9");
    expect(pitchLabel(69.5)).toBe("A#4 -50¢");
    expect(pitchLabel(69.3)).toBe("A4 +30¢");
    expect(pitchLabel(68.7)).toBe("A4 -30¢");
    expect(pitchLabel(69.04)).toBe("A4");
    expect(pitchLabel(NaN)).toBe("?");
    expect(pitchLabel(-3)).toBe("A-2");
  });

  it("44.2 shows at least an octave of rows around the notes, and a default when empty", () => {
    expect(rollRange([])).toEqual({ low: 60, high: 72, rows: 13 });
    const wide = rollRange([[0, 10, 40], [0, 10, 70]]);
    expect(wide).toEqual({ low: 39, high: 71, rows: 33 });
    const narrow = rollRange([[0, 10, 60]]);
    expect(narrow.high - narrow.low).toBeGreaterThanOrEqual(12);
    expect(narrow.low).toBeLessThanOrEqual(59);
    expect(narrow.high).toBeGreaterThanOrEqual(61);
    expect(rollRange([[0, 1, NaN]])).toEqual({ low: 60, high: 72, rows: 13 });
    expect(rollHeight(wide)).toBe(33 * ROW_HEIGHT);
  });

  it("44.3 puts a note by tick and pitch, with a quarter-tone note between two rows", () => {
    const roll = { low: 58, high: 72, rows: 15 };
    const on: NoteTuple = [0, 500, 70];
    const half: NoteTuple = [500, 500, 70.5];
    const a = noteBox(on, 1000, 544, roll);
    const b = noteBox(half, 1000, 544, roll);
    expect(a.x).toBe(KEY_WIDTH);
    expect(a.w).toBeCloseTo(250);
    expect(b.x).toBeCloseTo(KEY_WIDTH + 250);
    expect(a.y).toBe(2 * ROW_HEIGHT);
    expect(b.y).toBeCloseTo(1.5 * ROW_HEIGHT);
    // Past the end and before the start are clamped; a tiny note stays visible.
    const out = noteBox([-50, 5000, 60], 1000, 544, roll);
    expect(out.x).toBe(KEY_WIDTH);
    expect(out.x + out.w).toBeCloseTo(544);
    expect(noteBox([10, 0, 60], 1000, 544, roll).w).toBeGreaterThanOrEqual(2);
    expect(noteBox([0, 10, 60], 0, 544, roll).x).toBe(KEY_WIDTH);
  });

  it("44.4 finds the note under the pointer, preferring the later of overlapping notes", () => {
    const roll = { low: 58, high: 72, rows: 15 };
    const notes: NoteTuple[] = [[0, 600, 70], [400, 600, 70], [0, 100, 65]];
    const at = (tick: number, pitch: number) =>
      noteAt(KEY_WIDTH + (tick / 1000) * 500 + 1, (roll.high - pitch) * ROW_HEIGHT + 3, notes, 1000, 544, roll);
    expect(at(10, 70)).toBe(0);
    expect(at(500, 70)).toBe(1);
    expect(at(10, 65)).toBe(2);
    expect(at(800, 65)).toBe(-1);
    expect(noteAt(5, 5, notes, 1000, 544, roll)).toBe(-1);
    expect(noteAt(100, 100, [], 1000, 544, roll)).toBe(-1);
  });
});

import { thumbBoxes } from "./pianoroll";

describe("take thumbnails", () => {
  it("45.5 draws each note inside the box, and nothing for empty or unusable input", () => {
    const boxes = thumbBoxes([[0, 500, 60], [500, 500, 72]], 1000, 200, 40);
    expect(boxes).toHaveLength(2);
    expect(boxes[0].x).toBe(0);
    expect(boxes[0].w).toBeCloseTo(100);
    expect(boxes[1].x).toBeCloseTo(100);
    expect(boxes[0].y).toBeGreaterThan(boxes[1].y);
    for (const b of boxes) {
      expect(b.y).toBeGreaterThanOrEqual(0);
      expect(b.y + b.h).toBeLessThanOrEqual(40.0001);
      expect(b.x + b.w).toBeLessThanOrEqual(200.0001);
    }
    expect(thumbBoxes([], 1000, 200, 40)).toEqual([]);
    expect(thumbBoxes([[0, 1, 60]], 0, 200, 40)).toEqual([]);
    expect(thumbBoxes([[0, 1, 60]], 1000, 0, 40)).toEqual([]);
    expect(thumbBoxes([[0, 1, NaN]], 1000, 200, 40)).toEqual([]);
    expect(thumbBoxes([[0, 1, 60]], 1000, 200, 40)).toHaveLength(1);
  });
});

describe("piano roll robustness", () => {
  it("46.3 / 47.1 huge pitches, huge note counts and non-finite ticks cannot blow up the roll or a thumbnail", () => {
    const wild = rollRange([[0, 1, -1e9], [0, 1, 1e9], [0, 1, 60]]);
    expect(wild).toEqual(rollRange([[0, 1, 60]]));
    expect(wild.rows).toBeLessThanOrEqual(200);
    const many: NoteTuple[] = Array.from({ length: 300_000 }, (_, i) => [i, 1, 40 + (i % 30)]);
    expect(rollRange(many)).toEqual({ low: 39, high: 70, rows: 32 });
    expect(thumbBoxes(many, 300_000, 200, 40)).toHaveLength(300_000);
    expect(thumbBoxes([[NaN, 1, 60], [Infinity, 1, 60], [0, NaN, 60], [0, 1, 1e9]], 1000, 200, 40)).toEqual([]);
    const roll = rollRange([[0, 1, 60]]);
    for (const pitch of [1e9, -1e9, NaN]) {
      const b = noteBox([0, 10, pitch], 1000, 544, roll);
      expect(Number.isFinite(b.x) && Number.isFinite(b.w)).toBe(true);
      expect(noteAt(KEY_WIDTH + 1, 5, [[0, 10, pitch]], 1000, 544, roll)).toBe(-1);
    }
    const edge = rollRange([[0, 1, 160], [0, 1, -24]]);
    expect(edge.rows).toBeLessThanOrEqual(190);
  });
});
