import { describe, expect, it } from "vitest";
import {
  FIRST_GUESS_MODEL, estimateSeconds, fraction, recordDuration, remainingLabel, stageLabel,
} from "./progress";

describe("compose progress", () => {
  it("60.2 estimates from the median of the last five real durations, else a first guess", () => {
    expect(estimateSeconds([], FIRST_GUESS_MODEL)).toBe(30);
    expect(estimateSeconds([40], 30)).toBe(40);
    expect(estimateSeconds([10, 50, 20], 30)).toBe(20);
    expect(estimateSeconds([10, 20, 30, 40], 30)).toBe(25);
    expect(estimateSeconds([1000, 1000, 1000, 5, 5, 5, 5, 5], 30)).toBe(5);
    expect(estimateSeconds([NaN, -3, 0], 30)).toBe(30);
    expect(estimateSeconds([900], 30)).toBe(300);
  });

  it("60.3 keeps the last five durations and ignores impossible ones", () => {
    let h: number[] = [];
    for (const s of [1, 2, 3, 4, 5, 6]) h = recordDuration(h, s);
    expect(h).toEqual([2, 3, 4, 5, 6]);
    for (const bad of [NaN, Infinity, 0, -1, 601]) expect(recordDuration(h, bad)).toBe(h);
  });

  it("60.4 fills steadily to 90% at the expected time, then creeps on and never reaches 100%", () => {
    expect(fraction(0, 30)).toBe(0);
    expect(fraction(15, 30)).toBeCloseTo(0.45);
    expect(fraction(30, 30)).toBeCloseTo(0.9);
    expect(fraction(60, 30)).toBeGreaterThan(0.9);
    expect(fraction(60, 30)).toBeLessThan(0.99);
    expect(fraction(1e6, 30)).toBeLessThanOrEqual(0.99);
    let last = 0;
    for (let t = 0; t <= 200; t += 1) {
      const f = fraction(t, 30);
      expect(f).toBeGreaterThanOrEqual(last);
      last = f;
    }
    expect(fraction(-5, 30)).toBe(0);
    expect(fraction(5, 0)).toBe(0);
    expect(fraction(NaN, 30)).toBe(0);
  });

  it("60.5 says how long is left, and plainly when it is taking longer than usual", () => {
    expect(remainingLabel(0, 30)).toBe("about 30 s left");
    expect(remainingLabel(18.2, 30)).toBe("about 12 s left");
    expect(remainingLabel(30, 30)).toBe("taking longer than usual…");
    expect(remainingLabel(45, 30)).toBe("taking longer than usual…");
  });

  it("60.6 names each stage in words", () => {
    expect(stageLabel(null, null)).toBe("Starting…");
    expect(stageLabel({ stage: "asking", attempt: 1 }, "Anthropic (Claude) · claude-sonnet-4-5")).toBe(
      "Asking Anthropic (Claude) · claude-sonnet-4-5…",
    );
    expect(stageLabel({ stage: "asking", attempt: 1 }, null)).toBe("Asking the language model…");
    expect(stageLabel({ stage: "asking", attempt: 2 }, "x")).toBe("The first answer was unusable, asking again…");
    expect(stageLabel({ stage: "composing" }, "x")).toBe("Composing the notes…");
    expect(stageLabel({ stage: "saving" }, "x")).toBe("Saving to Your pieces…");
  });
});
