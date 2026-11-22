/** Where making a piece has got to, as sent by the app (`compose-progress` events). */
export type Stage =
  | { stage: "asking"; attempt: number }
  | { stage: "composing" }
  | { stage: "saving" };

/** First guesses before any piece has been timed: a language model, or the offline reader. */
export const FIRST_GUESS_MODEL = 30;
export const FIRST_GUESS_OFFLINE = 2;
const KEEP = 5;

/** The expected seconds: the median of the last few real durations, else the first guess. */
export function estimateSeconds(history: number[], firstGuess: number): number {
  const usable = history.filter((s) => Number.isFinite(s) && s > 0).slice(-KEEP);
  if (usable.length === 0) return firstGuess;
  const sorted = [...usable].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
  return Math.min(300, Math.max(1, median));
}

/** Adds a finished duration, keeping the last five; impossible values are ignored. */
export function recordDuration(history: number[], seconds: number): number[] {
  if (!Number.isFinite(seconds) || seconds <= 0 || seconds > 600) return history;
  return [...history, seconds].slice(-KEEP);
}

/**
 * How full the bar is (0 to 1). It moves steadily to 90% at the expected time, then creeps on
 * towards 99% for as long as it takes, so it never stalls and never claims to be done early.
 */
export function fraction(elapsed: number, estimate: number): number {
  if (!(elapsed > 0) || !(estimate > 0)) return 0;
  if (elapsed <= estimate) return 0.9 * (elapsed / estimate);
  return Math.min(0.99, 0.9 + 0.09 * (1 - Math.exp(-(elapsed - estimate) / estimate)));
}

/** "about 12 s left", or a plain word that it is taking longer than usual. */
export function remainingLabel(elapsed: number, estimate: number): string {
  const left = Math.ceil(estimate - elapsed);
  if (left >= 1) return `about ${left} s left`;
  return "taking longer than usual…";
}

/** What the app is doing, in words. */
export function stageLabel(stage: Stage | null, model: string | null): string {
  if (!stage) return "Starting…";
  switch (stage.stage) {
    case "asking":
      return stage.attempt > 1
        ? "The first answer was unusable, asking again…"
        : `Asking ${model ?? "the language model"}…`;
    case "composing":
      return "Composing the notes…";
    case "saving":
      return "Saving to Your pieces…";
  }
}
