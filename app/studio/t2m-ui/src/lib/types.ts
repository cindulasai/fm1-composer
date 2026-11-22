// The shapes `t2m-session` serialises. Kept by hand until generated types land.
export interface GalleryItem {
  id: string;
  title: string;
  style: string;
}
export interface SectionView {
  id: string;
  start: number;
  end: number;
}
/** [start tick, length in ticks, MIDI note number with cents as a fraction] */
export type NoteTuple = [number, number, number];
export interface TrackView {
  id: string;
  role: string;
  instrument: string;
  locked: boolean;
  notes: NoteTuple[];
}
export interface View {
  gallery_id: string;
  title: string;
  style: string;
  tempo_bpm: number;
  seed: number;
  ticks_per_quarter: number;
  end: number;
  sections: SectionView[];
  tracks: TrackView[];
  failed_gates: string[];
  can_undo: boolean;
  can_redo: boolean;
}
export interface Transport {
  playing: boolean;
  /** Position in ticks. */
  position: number;
}
export interface PromptResult {
  view: View;
  source: "cloud" | "offline";
  note: string | null;
}
export interface MadePiece {
  name: string;
  title: string;
  source: "cloud" | "offline";
  note: string | null;
  model: string | null;
}
export interface AiSettings {
  provider: string;
  model: string;
  base_url: string | null;
}
export interface AiProvider {
  id: string;
  label: string;
  key_url: string;
  needs_key: boolean;
  default_url: string | null;
  needs_url: boolean;
  models: string[];
}
export interface AiInfo {
  settings: AiSettings | null;
  has_key: boolean;
  active: string | null;
  providers: AiProvider[];
}
