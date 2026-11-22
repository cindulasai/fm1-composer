<script lang="ts">
  import type { View } from "./lib/types";
  import {
    HEADER_WIDTH, LANE_HEIGHT, RIBBON_HEIGHT, canvasHeight, laneAt, noteRect, pitchRange, sectionAt, tickToX,
  } from "./lib/layout";

  let { view, position, onseek, onreroll, onlock }: {
    view: View;
    position: number;
    onseek: (tick: number) => void;
    onreroll: (track: string, section: string | null) => void;
    onlock: (track: string, on: boolean) => void;
  } = $props();

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let width = $state(900);

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function draw() {
    if (!canvas) return;
    const height = canvasHeight(view.tracks.length);
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const g = canvas.getContext("2d");
    if (!g) return;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, width, height);
    g.font = "12px " + css("--font");
    g.textBaseline = "middle";

    // Form ribbon.
    view.sections.forEach((s, i) => {
      const x1 = tickToX(s.start, view.end, width);
      const x2 = tickToX(s.end, view.end, width);
      g.fillStyle = i % 2 ? css("--panel-2") : css("--ribbon");
      g.fillRect(x1, 0, x2 - x1, RIBBON_HEIGHT);
      g.fillStyle = css("--muted");
      g.fillText(s.id, x1 + 6, RIBBON_HEIGHT / 2);
    });

    // Lanes.
    view.tracks.forEach((track, i) => {
      const top = RIBBON_HEIGHT + i * LANE_HEIGHT;
      g.fillStyle = i % 2 ? css("--lane-b") : css("--lane-a");
      g.fillRect(0, top, width, LANE_HEIGHT);
      g.fillStyle = css("--text");
      g.fillText(track.id, 10, top + 18);
      g.fillStyle = css("--muted");
      g.fillText(track.locked ? "locked" : track.instrument, 10, top + 40);
      const kit = track.role === "drums" || track.role === "percussion";
      const range = pitchRange(track.notes);
      g.fillStyle = track.locked ? css("--note-locked") : kit ? css("--note-kit") : css("--note");
      for (const n of track.notes) {
        const r = noteRect(n, view.end, width, top, range);
        g.fillRect(r.x, r.y, r.w, r.h);
      }
    });

    // Section borders.
    g.strokeStyle = css("--line");
    g.lineWidth = 1;
    for (const s of view.sections) {
      const x = Math.round(tickToX(s.start, view.end, width)) + 0.5;
      g.beginPath();
      g.moveTo(x, 0);
      g.lineTo(x, height);
      g.stroke();
    }
  }

  $effect(() => {
    view;
    width;
    draw();
  });

  $effect(() => {
    const observer = new ResizeObserver(() => {
      width = Math.max(600, Math.floor(host.clientWidth));
    });
    observer.observe(host);
    return () => observer.disconnect();
  });

  function click(event: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (y < RIBBON_HEIGHT && x >= HEADER_WIDTH) {
      const at = sectionAt(x, view.sections, view.end, width);
      const found = view.sections.find((s) => s.id === at);
      if (found) onseek(found.start);
      return;
    }
    const lane = laneAt(y, view.tracks.length);
    if (lane < 0) return;
    const track = view.tracks[lane];
    if (x < HEADER_WIDTH) {
      onlock(track.id, !track.locked);
      return;
    }
    const section = sectionAt(x, view.sections, view.end, width);
    if (section) onreroll(track.id, section);
  }
</script>

<div class="host" bind:this={host}>
  <div class="wrap">
    <div class="playhead" style:left="{tickToX(position, view.end, width)}px" style:height="{canvasHeight(view.tracks.length)}px" hidden={position <= 0}></div>
  <canvas bind:this={canvas} onclick={click} aria-label="Arrangement: click a part's name to lock it, click its notes in a section to reroll that section"></canvas>
  </div>
</div>

<style>
  .host { width: 100%; overflow: auto; border: 1px solid var(--line); border-radius: var(--radius); }
  .wrap { position: relative; }
  .playhead { position: absolute; top: 0; width: 2px; background: var(--accent); pointer-events: none; }
  canvas { display: block; cursor: pointer; }
</style>
