<script lang="ts">
  import { api } from "./lib/api";
  import type { TrackView, View } from "./lib/types";
  import { KEY_WIDTH, ROW_HEIGHT, noteBox, pitchLabel, rollHeight, rollRange, thumbBoxes } from "./lib/pianoroll";

  let { view, position, onkeep }: {
    view: View;
    position: number;
    onkeep: (track: string, take: number) => void;
  } = $props();

  let takes = $state<TrackView[]>([]);
  let takesFor = $state("");
  let takesError = $state("");

  // Takes belong to the piece they were made from: any change to the piece drops them, and an
  // answer still on its way for an older piece is ignored when it arrives.
  let generation = 0;
  $effect(() => {
    void view;
    void chosen;
    generation += 1;
    takes = [];
    takesError = "";
  });

  async function tryTakes() {
    if (!track) return;
    takesError = "";
    const mine = generation;
    const id = track.id;
    try {
      const got = await api.takes(id, null, 3);
      if (mine !== generation) return;
      takes = got;
      takesFor = id;
    } catch (error) {
      if (mine !== generation) return;
      takes = [];
      takesError = String(error);
    }
  }

  function thumb(node: HTMLCanvasElement, notes: TrackView["notes"]) {
    function paint(current: TrackView["notes"]) {
      const g = node.getContext("2d");
      if (!g) return;
      g.clearRect(0, 0, node.width, node.height);
      g.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--note").trim();
      for (const b of thumbBoxes(current, view.end, node.width, node.height)) g.fillRect(b.x, b.y, b.w, b.h);
    }
    paint(notes);
    return { update: paint };
  }

  let chosen = $state("");
  const track = $derived(view.tracks.find((t) => t.id === chosen) ?? view.tracks[0]);
  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let width = $state(900);

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function draw() {
    if (!canvas || !track) return;
    const roll = rollRange(track.notes);
    const height = rollHeight(roll);
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const g = canvas.getContext("2d");
    if (!g) return;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, width, height);
    g.font = "10px " + css("--font");
    g.textBaseline = "middle";
    for (let p = roll.high; p >= roll.low; p--) {
      const y = (roll.high - p) * ROW_HEIGHT;
      const black = [1, 3, 6, 8, 10].includes(((p % 12) + 12) % 12);
      g.fillStyle = black ? css("--lane-b") : css("--lane-a");
      g.fillRect(KEY_WIDTH, y, width - KEY_WIDTH, ROW_HEIGHT);
      if (p % 12 === 0 || roll.rows < 30) {
        g.fillStyle = css("--muted");
        g.fillText(pitchLabel(p), 4, y + ROW_HEIGHT / 2);
      }
    }
    g.fillStyle = track.locked ? css("--note-locked") : css("--note");
    for (const n of track.notes) {
      const b = noteBox(n, view.end, width, roll);
      g.fillRect(b.x, b.y + 1, b.w, b.h);
    }
    if (view.end > 0) {
      const x = KEY_WIDTH + (Math.min(position, view.end) / view.end) * (width - KEY_WIDTH);
      g.fillStyle = css("--accent");
      g.fillRect(x, 0, 1.5, height);
    }
  }

  $effect(() => {
    void view; void position; void width; void track;
    draw();
  });

  $effect(() => {
    if (!host) return;
    const watch = new ResizeObserver(() => { width = Math.max(300, host.clientWidth); });
    watch.observe(host);
    width = Math.max(300, host.clientWidth);
    return () => watch.disconnect();
  });
</script>

<section class="roll" aria-label="Piano roll">
  <header>
    <label>Piano roll
      <select bind:value={chosen}>
        {#each view.tracks as t}<option value={t.id}>{t.id} ({t.instrument})</option>{/each}
      </select>
    </label>
    <span class="count">{track ? track.notes.length : 0} notes</span>
    <button onclick={tryTakes} disabled={!track || track.locked}>Try 3 takes</button>
  </header>
  {#if takesError}<p class="err" role="alert">{takesError}</p>{/if}
  {#if takes.length}
    <ul class="takes" aria-label="Takes of {takesFor}">
      {#each takes as take, i}
        <li>
          <canvas width="220" height="36" use:thumb={take.notes}></canvas>
          <span>Take {i + 1} · {take.notes.length} notes</span>
          <button onclick={() => onkeep(takesFor, i + 1)}>Keep</button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="host" bind:this={host}><canvas bind:this={canvas}></canvas></div>
</section>

<style>
  .roll { margin-top: var(--gap); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: var(--gap); }
  header { display: flex; gap: var(--gap); align-items: center; margin-bottom: 6px; color: var(--muted); font-size: 13px; }
  select { background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 2px 6px; margin-left: 6px; }
  .takes { list-style: none; display: flex; gap: var(--gap); padding: 0; margin: 0 0 6px; flex-wrap: wrap; }
  .takes li { display: flex; flex-direction: column; gap: 4px; background: var(--panel-2); border-radius: var(--radius); padding: 6px; font-size: 12px; color: var(--muted); }
  .takes canvas { width: 220px; height: 36px; background: var(--lane-a); border-radius: 4px; }
  .err { color: var(--bad); margin: 0 0 6px; font-size: 13px; }
  button { background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 3px 10px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .host { max-height: 360px; overflow-y: auto; overflow-x: hidden; }
  canvas { display: block; }
</style>
