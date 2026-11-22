import { invoke } from "@tauri-apps/api/core";
import type { AiInfo, AiSettings, GalleryItem, MadePiece, TrackView, PromptResult, Transport, View } from "./types";

export const api = {
  gallery: () => invoke<GalleryItem[]>("gallery"),
  openExample: (id: string, seed: number) => invoke<View>("open_example", { id, seed }),
  openPrompt: (text: string, seed: number) => invoke<PromptResult>("open_prompt", { text, seed }),
  makePiece: (text: string, seed: number) => invoke<MadePiece>("make_piece", { text, seed }),
  aiSettings: () => invoke<AiInfo>("ai_settings"),
  saveAiSettings: (settings: AiSettings, key: string | null) =>
    invoke<AiInfo>("save_ai_settings", { settings, key }),
  removeAiKey: (provider: string) => invoke<AiInfo>("remove_ai_key", { provider }),
  testAi: () => invoke<string>("test_ai"),
  takes: (track: string, section: string | null, count: number) =>
    invoke<TrackView[]>("takes", { track, section, count }),
  keepTake: (track: string, section: string | null, take: number) =>
    invoke<View>("keep_take", { track, section, take }),
  mix: (muted: boolean[], soloed: boolean[], looping: boolean) =>
    invoke<void>("mix", { muted, soloed, looping }),
  reroll: (track: string, section: string | null) => invoke<View>("reroll", { track, section }),
  reseed: (seed: number) => invoke<View>("reseed", { seed }),
  lock: (track: string, on: boolean) => invoke<View>("lock", { track, on }),
  undo: () => invoke<View>("undo"),
  redo: () => invoke<View>("redo"),
  play: (from: number | null) => invoke<Transport>("play", { from }),
  stop: () => invoke<Transport>("stop"),
  transport: () => invoke<Transport>("transport"),
  saveProject: () => invoke<string>("save_project"),
  savedProjects: () => invoke<string[]>("saved_projects"),
  openProject: (name: string) => invoke<View>("open_project", { name }),
  exportMidi: (profile: string) => invoke<string[]>("export_midi", { profile }),
};
