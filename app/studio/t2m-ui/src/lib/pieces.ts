/** A saved file name shown as a readable title: "slow-sad-lofi-482910.t2m.json" → "Slow sad lofi". */
export function pieceLabel(name: string): string {
  const stem = name.replace(/\.t2m\.json$/, "").replace(/-\d+$/, "");
  const words = stem.replace(/[-_]+/g, " ").trim();
  if (!words) return name;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/**
 * Whether a piece that has just been made opens straight away. While music is playing it waits
 * in the list (marked new) so the playing piece is not interrupted; otherwise it opens.
 */
export function openNow(playing: boolean): boolean {
  return !playing;
}

/** The "new" marks after a piece is made (add it) or opened (remove it). */
export function markNew(marks: string[], name: string): string[] {
  return [name, ...marks.filter((m) => m !== name)];
}
export function clearNew(marks: string[], name: string): string[] {
  return marks.filter((m) => m !== name);
}
