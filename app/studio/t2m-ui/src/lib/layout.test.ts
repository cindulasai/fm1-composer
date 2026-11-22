import { describe, expect, it } from "vitest";
import {
  HEADER_WIDTH, LANE_HEIGHT, RIBBON_HEIGHT, canvasHeight, laneAt, noteRect, pitchRange, sectionAt, tickToX,
} from "./layout";

const sections = [
  { id: "a", start: 0, end: 1000 },
  { id: "b", start: 1000, end: 3000 },
];

describe("layout", () => {
  it("29.1 maps ticks to x inside the time area and clamps outside the piece", () => {
    expect(tickToX(0, 1000, 1168)).toBe(HEADER_WIDTH);
    expect(tickToX(1000, 1000, 1168)).toBe(1168);
    expect(tickToX(500, 1000, 1168)).toBe(HEADER_WIDTH + 500);
    expect(tickToX(-5, 1000, 1168)).toBe(HEADER_WIDTH);
    expect(tickToX(9999, 1000, 1168)).toBe(1168);
    expect(tickToX(10, 0, 1168)).toBe(HEADER_WIDTH);
    expect(tickToX(10, 1000, 100)).toBe(HEADER_WIDTH);
  });

  it("29.2 pads the pitch range and gives an empty lane a default", () => {
    expect(pitchRange([])).toEqual([60, 72]);
    expect(pitchRange([[0, 1, 60]])).toEqual([53, 67]);
    expect(pitchRange([[0, 1, 40], [0, 1, 80]])).toEqual([39, 81]);
  });

  it("29.3 puts higher notes higher up and never makes a note invisible", () => {
    const range: [number, number] = [50, 70];
    const low = noteRect([0, 100, 52], 1000, 1168, RIBBON_HEIGHT, range);
    const high = noteRect([0, 100, 68], 1000, 1168, RIBBON_HEIGHT, range);
    expect(high.y).toBeLessThan(low.y);
    const tiny = noteRect([0, 0, 60], 1000, 1168, RIBBON_HEIGHT, range);
    expect(tiny.w).toBeGreaterThanOrEqual(1.5);
    for (const r of [low, high, tiny]) {
      expect(r.y).toBeGreaterThanOrEqual(RIBBON_HEIGHT);
      expect(r.y + r.h).toBeLessThanOrEqual(RIBBON_HEIGHT + LANE_HEIGHT);
    }
  });

  it("29.4 finds the section under a click, and none in the header or past the end", () => {
    expect(sectionAt(HEADER_WIDTH + 10, sections, 3000, 1168)).toBe("a");
    expect(sectionAt(tickToX(2000, 3000, 1168), sections, 3000, 1168)).toBe("b");
    expect(sectionAt(10, sections, 3000, 1168)).toBeNull();
    expect(sectionAt(1168, sections, 3000, 1168)).toBeNull();
    expect(sectionAt(500, [], 0, 1168)).toBeNull();
  });

  it("29.5 finds the lane under a click", () => {
    expect(laneAt(5, 3)).toBe(-1);
    expect(laneAt(RIBBON_HEIGHT + 1, 3)).toBe(0);
    expect(laneAt(RIBBON_HEIGHT + LANE_HEIGHT + 1, 3)).toBe(1);
    expect(laneAt(RIBBON_HEIGHT + 3 * LANE_HEIGHT, 3)).toBe(-1);
    expect(canvasHeight(3)).toBe(RIBBON_HEIGHT + 3 * LANE_HEIGHT);
  });
});

describe("playhead", () => {
  it("32.1 sits at the time area's start for 0, halfway for half, and stops at the right edge past the end", () => {
    expect(tickToX(0, 1000, 900)).toBe(HEADER_WIDTH);
    expect(tickToX(500, 1000, 900)).toBeCloseTo(HEADER_WIDTH + (900 - HEADER_WIDTH) / 2);
    expect(tickToX(5000, 1000, 900)).toBe(900);
    expect(tickToX(-5, 1000, 900)).toBe(HEADER_WIDTH);
  });
});

