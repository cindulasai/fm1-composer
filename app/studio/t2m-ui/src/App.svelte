<script lang="ts">
  import { onMount } from "svelte";
  import Arrangement from "./Arrangement.svelte";
  import PianoRoll from "./PianoRoll.svelte";
  import Settings from "./Settings.svelte";
  import { api } from "./lib/api";
  import { clearNew, markNew, openNow, pieceLabel } from "./lib/pieces";
  import { listen } from "@tauri-apps/api/event";
  import {
    FIRST_GUESS_MODEL, FIRST_GUESS_OFFLINE, estimateSeconds, fraction, recordDuration, remainingLabel, stageLabel,
    type Stage,
  } from "./lib/progress";
  import type { GalleryItem, View } from "./lib/types";

  let gallery = $state<GalleryItem[]>([]);
  let view = $state<View | null>(null);
  let muted = $state<boolean[]>([]);
  let soloed = $state<boolean[]>([]);
  let looping = $state(false);
  let mixFor = "";

  // A different piece starts with every part on and no loop; the engine is told whenever this changes.
  $effect(() => {
    if (!view) return;
    const n = view.tracks.length;
    if (mixFor !== view.gallery_id || muted.length !== n) {
      mixFor = view.gallery_id;
      muted = Array(n).fill(false);
      soloed = Array(n).fill(false);
      looping = false;
    }
  });
  $effect(() => {
    const m = [...muted];
    const s = [...soloed];
    const l = looping;
    if (!view) return;
    api.mix(m, s, l).catch(() => {});
  });
  function toggle(list: boolean[], i: number): boolean[] {
    const next = [...list];
    next[i] = !next[i];
    return next;
  }
  let filter = $state("");
  let profile = $state("mpe");
  let message = $state("");
  let failed = $state(false);
  let busy = $state(false);
  let saved = $state<string[]>([]);
  let prompt = $state("");
  let showSettings = $state(false);
  let activeModel = $state<string | null>(null);
  let fresh = $state<string[]>([]);
  let openName = $state<string | null>(null);
  let making = $state(false);
  let stage = $state<Stage | null>(null);
  let elapsed = $state(0);
  let estimate = $state(FIRST_GUESS_MODEL);
  const filled = $derived(Math.round(fraction(elapsed, estimate) * 100));

  /** Past durations per model, kept in this browser only (a convenience; losing them is harmless). */
  function history(key: string): number[] {
    try {
      const raw = JSON.parse(localStorage.getItem(`t2m.durations.${key}`) ?? "[]");
      return Array.isArray(raw) ? raw.filter((n) => typeof n === "number") : [];
    } catch {
      return [];
    }
  }
  function remember(key: string, seconds: number) {
    try {
      localStorage.setItem(`t2m.durations.${key}`, JSON.stringify(recordDuration(history(key), seconds)));
    } catch {
      /* storage unavailable: estimates just stay at the first guess */
    }
  }
  /** Counts pieces opened or changed, so a finished piece never replaces one opened meanwhile. */
  let changes = 0;
  let playing = $state(false);
  let position = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  const shown = $derived(
    gallery.filter((g) => (g.title + " " + g.style + " " + g.id).toLowerCase().includes(filter.toLowerCase())),
  );

  function say(text: string, bad = false) {
    message = text;
    failed = bad;
  }

  /** Runs one action on the open piece; true when it went through. */
  async function step(action: () => Promise<View>, keep = false): Promise<boolean> {
    if (busy) return false;
    busy = true;
    try {
      view = await action();
      changes += 1;
      say("");
      if (keep) await autoSave();
      return true;
    } catch (error) {
      say(String(error), true);
      return false;
    } finally {
      busy = false;
    }
  }

  /** A newly made piece goes into the Saved list at once, so nothing has to be saved by hand. */
  async function autoSave() {
    try {
      await api.saveProject();
      await refreshSaved();
    } catch (error) {
      say(`The piece was made but could not be saved: ${error}`, true);
    }
  }

  async function togglePlay() {
    try {
      const t = playing ? await api.stop() : await api.play(null);
      // The sound engine may have been created just now: tell it what the strip shows.
      if (!playing) await api.mix([...muted], [...soloed], looping).catch(() => {});
      playing = t.playing;
      position = t.position;
      if (playing && !timer) timer = setInterval(poll, 50);
      say("");
    } catch (error) {
      say(String(error), true);
    }
  }

  async function poll() {
    try {
      const t = await api.transport();
      playing = t.playing;
      position = t.position;
      if (!playing && timer) {
        clearInterval(timer);
        timer = undefined;
      }
    } catch {
      playing = false;
      if (timer) clearInterval(timer);
      timer = undefined;
    }
  }

  async function seek(tick: number) {
    try {
      const t = await api.play(tick);
      playing = t.playing;
      position = t.position;
      if (!timer) timer = setInterval(poll, 50);
    } catch (error) {
      say(String(error), true);
    }
  }

  async function refreshSaved() {
    try {
      saved = await api.savedProjects();
    } catch {
      saved = [];
    }
  }

  async function saveProject() {
    try {
      const name = await api.saveProject();
      say(`Saved ${name}`);
      await refreshSaved();
    } catch (error) {
      say(String(error), true);
    }
  }

  /**
   * Makes a piece from the description in the background. It always lands in "Your pieces";
   * it opens at once only when nothing is playing, so the music you are listening to is never
   * cut off. Several can be made in a row.
   */
  async function describe() {
    const text = prompt.trim();
    if (!text) {
      say("Describe the music in a few words first, for example: slow sad lofi in D minor at 80 bpm", true);
      return;
    }
    if (making) return;
    making = true;
    const before = changes;
    const key = activeModel ?? "offline";
    estimate = estimateSeconds(history(key), activeModel ? FIRST_GUESS_MODEL : FIRST_GUESS_OFFLINE);
    stage = null;
    elapsed = 0;
    say("");
    const started = performance.now();
    const tick = setInterval(() => (elapsed = (performance.now() - started) / 1000), 200);
    const stopListening = await listen<Stage>("compose-progress", (event) => (stage = event.payload)).catch(() => () => {});
    try {
      const made = await api.makePiece(text, newSeed());
      // Timed under the reader that actually made it, so a failed model call does not skew it.
      remember(made.model ?? "offline", (performance.now() - started) / 1000);
      await refreshSaved();
      const how = made.note ?? (made.model ? `Written with ${made.model}.` : "");
      if (openNow(playing) && changes === before && (await openPiece(made.name))) {
        say(how);
      } else {
        fresh = markNew(fresh, made.name);
        say(`"${made.title}" is ready under Your pieces; click it to open. ${how}`.trim());
      }
    } catch (error) {
      say(String(error), true);
    } finally {
      clearInterval(tick);
      stopListening();
      making = false;
      stage = null;
    }
  }

  async function openPiece(name: string): Promise<boolean> {
    const opened = await step(() => api.openProject(name));
    if (opened) {
      openName = name;
      fresh = clearNew(fresh, name);
    }
    return opened;
  }

  async function loadAi() {
    try {
      activeModel = (await api.aiSettings()).active;
    } catch {
      activeModel = null;
    }
  }

  function newSeed(): number {
    return Math.floor(Math.random() * 1_000_000);
  }

  async function exportFiles() {
    try {
      const paths = await api.exportMidi(profile);
      say(`Saved ${paths.length} file${paths.length === 1 ? "" : "s"}: ${paths.join(", ")}`);
    } catch (error) {
      say(String(error), true);
    }
  }

  function keys(event: KeyboardEvent) {
    if (event.code === "Space" && view && !(event.target instanceof HTMLTextAreaElement) && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLSelectElement)) {
      event.preventDefault();
      void togglePlay();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z" && view) {
      event.preventDefault();
      if (event.shiftKey) {
        if (view.can_redo) void step(api.redo);
      } else if (view.can_undo) void step(api.undo);
    }
  }

  onMount(async () => {
    try {
      gallery = await api.gallery();
      await refreshSaved();
      await loadAi();
    } catch (error) {
      say(String(error), true);
    }
  });
</script>

<svelte:window onkeydown={keys} />

<div class="app">
  <aside>
    <div class="brand">
      <h1>text2midi</h1>
      <button class="gear" onclick={() => (showSettings = true)} aria-label="AI settings" title="AI settings">⚙</button>
    </div>
    <form class="describe" onsubmit={(e) => { e.preventDefault(); void describe(); }}>
      <textarea rows="3" placeholder="Describe the music: slow sad lofi in D minor at 80 bpm" bind:value={prompt} aria-label="Describe the music" maxlength="2000"></textarea>
      <button class="primary" type="submit" disabled={making}>{making ? "Composing…" : "Make it"}</button>
    </form>
    {#if making}
      <div class="progress" role="progressbar" aria-label="Composing" aria-valuemin="0" aria-valuemax="100" aria-valuenow={filled}>
        <div class="track"><span style="width: {filled}%"></span></div>
        <p>{stageLabel(stage, activeModel)}</p>
        <p class="times">{Math.floor(elapsed)} s · {remainingLabel(elapsed, estimate)}</p>
      </div>
    {/if}
    <p class="note">
      {#if activeModel}Using {activeModel}{:else}No AI set up: a plain keyword reader is used.{/if}
      <button class="link" onclick={() => (showSettings = true)}>{activeModel ? "Change" : "Set up AI"}</button>
    </p>
    <input type="search" placeholder="Filter examples" bind:value={filter} aria-label="Filter examples" />
    <ul>
      {#each shown as item (item.id)}
        <li>
          <button class:active={view?.gallery_id === item.id} onclick={() => { openName = null; void step(() => api.openExample(item.id, 1)); }}>
            <span class="title">{item.title || item.id}</span>
            <span class="style">{item.style}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if saved.length}
      <h3>Your pieces</h3>
      <ul>
        {#each saved as name (name)}
          <li>
            <button class:active={openName === name} onclick={() => openPiece(name)} title={name}>
              <span class="title">{pieceLabel(name)}{#if fresh.includes(name)} <span class="badge">New</span>{/if}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <main>
    {#if view}
      <header>
        <div>
          <h2>{view.title}</h2>
          <p class="meta">{view.style} · {view.tempo_bpm} bpm · seed {view.seed}</p>
        </div>
        <div class="actions">
          <button class="primary" onclick={togglePlay} aria-label={playing ? "Stop" : "Play"}>{playing ? "Stop" : "Play"}</button>
          <button onclick={() => step(api.undo)} disabled={!view.can_undo || busy}>Undo</button>
          <button onclick={() => step(api.redo)} disabled={!view.can_redo || busy}>Redo</button>
          <button class="primary" onclick={() => step(() => api.reseed(newSeed()), true)} disabled={busy}>New take</button>
          <select bind:value={profile} aria-label="How quarter-tones are written">
            <option value="mpe">MPE bends</option>
            <option value="tuning">Tuning files</option>
            <option value="approx">Nearest note</option>
          </select>
          <button onclick={saveProject}>Save</button>
          <button onclick={exportFiles}>Export MIDI</button>
        </div>
      </header>
      {#if view.failed_gates.length}
        <p class="warn">Quality check failed: {view.failed_gates.join(", ")}</p>
      {/if}
      <Arrangement
        {view}
        {position}
        onseek={seek}
        onreroll={(track, section) => step(() => api.reroll(track, section))}
        onlock={(track, on) => step(() => api.lock(track, on))}
      />
      <div class="mixer" role="group" aria-label="Mute, solo and loop">
        {#each view.tracks as t, i}
          <span class="strip">
            <span class="name">{t.id}</span>
            <button class:on={muted[i]} aria-pressed={!!muted[i]} aria-label="Mute {t.id}" onclick={() => (muted = toggle(muted, i))}>M</button>
            <button class:on={soloed[i]} aria-pressed={!!soloed[i]} aria-label="Solo {t.id}" onclick={() => (soloed = toggle(soloed, i))}>S</button>
          </span>
        {/each}
        <label class="loop"><input type="checkbox" bind:checked={looping} /> Loop</label>
      </div>
      <PianoRoll {view} {position} onkeep={(track, take) => step(() => api.keepTake(track, null, take))} />
      <p class="hint">Space plays and stops. Click the ribbon to play from a section. Click a part's name to lock it. Click inside a part's lane to get a new take of that part in that section.</p>
    {:else}
      <div class="empty">
        <h2>Choose an example</h2>
        <p>Pick a piece on the left. You can reroll any part, lock the parts you like, and export a MIDI file for your DAW.</p>
      </div>
    {/if}
    {#if message}
      <p class="message" class:bad={failed} role="status">{message}</p>
    {/if}
  </main>
</div>

{#if showSettings}
  <Settings onclose={() => (showSettings = false)} onchange={(active) => (activeModel = active)} />
{/if}

<style>
  .progress { display: flex; flex-direction: column; gap: 3px; margin: 2px 0 6px; font-size: 12px; color: var(--muted); }
  .progress p { margin: 0; }
  .progress .times { font-variant-numeric: tabular-nums; }
  .track { height: 6px; background: var(--panel-2); border-radius: 3px; overflow: hidden; }
  .track span { display: block; height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.2s linear; }
  @media (prefers-reduced-motion: reduce) { .track span { transition: none; } }
  .brand { display: flex; align-items: center; justify-content: space-between; }
  .gear { background: none; border: 1px solid var(--line); color: var(--text); border-radius: var(--radius); font-size: 16px; line-height: 1; padding: 4px 7px; cursor: pointer; }
  .gear:hover { border-color: var(--accent); }
  .link { background: none; border: none; color: var(--accent); cursor: pointer; padding: 0 0 0 4px; font-size: inherit; text-decoration: underline; }
  .badge { background: var(--accent); color: var(--accent-ink); border-radius: 8px; font-size: 10px; padding: 1px 6px; margin-left: 4px; vertical-align: middle; }
  .mixer { display: flex; flex-wrap: wrap; gap: var(--gap); align-items: center; margin: var(--gap) 0; color: var(--muted); font-size: 13px; }
  .strip { display: inline-flex; gap: 4px; align-items: center; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 2px 6px; }
  .strip .name { color: var(--text); margin-right: 2px; }
  .strip button { min-width: 24px; padding: 1px 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: 4px; cursor: pointer; }
  .strip button.on { background: var(--accent); color: var(--accent-ink); }
  :global(html, body, #app) { height: 100%; margin: 0; }
  :global(body) { background: var(--bg); color: var(--text); font-family: var(--font); font-size: 14px; }
  .app { display: grid; grid-template-columns: 260px 1fr; height: 100%; }
  aside { background: var(--panel); border-right: 1px solid var(--line); padding: var(--gap) 12px; overflow: auto; }
  .describe { display: flex; flex-direction: column; gap: 6px; margin-bottom: 6px; }
  textarea { width: 100%; box-sizing: border-box; padding: 6px 8px; background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); color: var(--text); font: inherit; resize: vertical; }
  .note { margin: 0 0 var(--gap); color: var(--muted); font-size: 11px; }
  h3 { font-size: 12px; margin: 16px 0 6px; color: var(--muted); text-transform: uppercase; }
  h1 { font-size: 16px; margin: 8px 0 12px; }
  input[type="search"] { width: 100%; box-sizing: border-box; padding: 6px 8px; margin-bottom: var(--gap);
    background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); color: var(--text); }
  ul { list-style: none; margin: 0; padding: 0; }
  li button { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 2px; padding: 8px;
    background: transparent; border: 0; border-radius: var(--radius); color: var(--text); cursor: pointer; }
  li button:hover { background: var(--panel-2); }
  li button.active { background: var(--accent); color: var(--accent-ink); }
  .title { font-weight: 600; }
  .style { font-size: 12px; opacity: 0.75; }
  main { padding: 16px 20px; overflow: auto; display: flex; flex-direction: column; gap: 12px; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; flex-wrap: wrap; }
  h2 { margin: 0; font-size: 20px; }
  .meta { margin: 4px 0 0; color: var(--muted); }
  .actions { display: flex; gap: var(--gap); align-items: center; flex-wrap: wrap; }
  button, select { padding: 6px 12px; border-radius: var(--radius); border: 1px solid var(--line);
    background: var(--panel); color: var(--text); cursor: pointer; font: inherit; }
  button:disabled { opacity: 0.45; cursor: default; }
  button.primary { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
  .warn { margin: 0; color: var(--warn); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .empty { margin: auto; max-width: 420px; text-align: center; color: var(--muted); }
  .message { margin: 0; padding: 8px 12px; border-radius: var(--radius); background: var(--panel-2); word-break: break-all; }
  .message.bad { color: var(--bad); }
</style>
