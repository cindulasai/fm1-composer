// The FM-1 on the page: fm1-emu's browser build (Joakim Eriksson, MIT; tools/get_web_emu.sh) running COMPOSER.
// A piece goes in over the emulated MIDI-in jack as COMPOSER's transfer SysEx, paced to the jack's 31250 baud by the
// emulator's own clock, then is confirmed by reading the library back from the emulated flash.

const MHZ = 160;
const BYTES_PER_MS = 3;                     // the jack: 31250 baud = 3.125 bytes a millisecond
const LIB_AT = 0x97000, LIB_SECT = 73, LIB_MAGIC = 0x3142494c;   // COMPOSER's library (firmware/src/store.h, lib.c)
const STORE = 'composer-studio.flash';
const BUTTONS = [['OCT-', 0], ['OCT+', 1], ['FX', 2], ['SEL', 3], ['EDIT', 6], ['HOME', 8], ['SAVE', 9], ['PLAY', 12]];
const ENCODERS = [['SELECT', 0], ['KNOB1', 2], ['KNOB2', 3], ['KNOB3', 4], ['PRESETS', 6]];
const SHORTCUTS = { ' ': ['key', 12], Escape: ['key', 8], ArrowRight: ['enc', 0, 1], ArrowLeft: ['enc', 0, -1],
                    ArrowDown: ['enc', 0, 1], ArrowUp: ['enc', 0, -1], e: ['key', 6], s: ['key', 9], z: ['key', 2] };

const CRC = new Uint32Array(256).map((_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
export function crc32(b) { let c = ~0; for (const x of b) c = CRC[(c ^ x) & 255] ^ (c >>> 8); return ~c >>> 0; }

function pack7(b) {
  const out = [];
  for (let i = 0; i < b.length; i += 7) {
    const g = b.subarray(i, i + 7);
    out.push(g.reduce((m, x, k) => m | (((x >> 7) & 1) << k), 0));
    for (const x of g) out.push(x & 0x7f);
  }
  return out;
}

function frame(cmd, payload = new Uint8Array(0)) {    // as app/tools/fm1c/link.py's frame()
  const p = new Uint8Array(payload.length + 4);
  p.set(payload);
  new DataView(p.buffer).setUint32(payload.length, crc32(payload), true);
  return [0xf0, 0x7d, 0x46, 0x31, cmd, ...pack7(p), 0xf7];
}

const u32 = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; };

/** every SysEx byte of one piece's transfer: PUT, the chunks, COMMIT */
export function transferBytes(piece) {
  const out = frame(3, u32(piece.length));
  for (let off = 0; off < piece.length; off += 256) {
    const c = new Uint8Array(4 + Math.min(256, piece.length - off));
    c.set(u32(off));
    c.set(piece.subarray(off, off + 256), 4);
    out.push(...frame(4, c));
  }
  out.push(...frame(5));
  return Uint8Array.from(out);
}

/** the pieces in a flash image, newest first: {id, seq, name, bytes} (deleted, damaged or foreign data skipped) */
export function library(flash) {
  const v = new DataView(flash.buffer, flash.byteOffset, flash.byteLength), out = [];
  for (let s = 0; s < LIB_SECT; s++) {
    const o = LIB_AT + s * 4096;
    if (v.getUint32(o, true) !== LIB_MAGIC || v.getUint32(o + 48, true) !== 0xffffffff) continue;
    const len = v.getUint32(o + 16, true);
    if (len < 1 || len > 32768 || o + 64 + len > flash.length) continue;
    const bytes = flash.slice(o + 64, o + 64 + len);
    if (crc32(bytes) !== v.getUint32(o + 20, true)) continue;
    const raw = flash.subarray(o + 24, o + 48);
    const name = String.fromCharCode(...raw.subarray(0, raw.indexOf(0) < 0 ? 24 : raw.indexOf(0)));
    out.push({ id: v.getUint32(o + 8, true), seq: v.getUint32(o + 12, true), name, bytes });
  }
  return out.sort((a, b) => b.seq - a.seq);
}

function b64(bytes) { let s = ''; for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000)); return btoa(s); }
function unb64(s) { return Uint8Array.from(atob(s), (c) => c.charCodeAt(0)); }

const LUT = new Uint32Array(65536).map((_, p) =>
  0xff000000 | (Math.round((p & 31) * 255 / 31) << 16) | (Math.round(((p >> 5) & 63) * 255 / 63) << 8) | Math.round((p >> 11) * 255 / 31));

export class Fm1 {
  constructor(box) {
    this.box = box;
    this.state = 'off';                       // off, booting, on, failed
    this.lcd = null;                          // the last LCD frame, RGB565 240 x 240
    this.ms = 0;                              // the emulator's clock
    this.waiters = [];                        // promises waiting for a frame or a snapshot
    this.pushing = false;
    this.level = 0;
    this.render();
  }

  el(tag, props = {}, ...kids) { const e = Object.assign(document.createElement(tag), props); e.append(...kids); return e; }

  render() {
    const b = this.box;
    b.replaceChildren();
    this.canvas = this.el('canvas', { width: 240, height: 240, id: 'emu-lcd', title: 'the FM-1\'s screen' });
    this.status = this.el('p', { id: 'emu-status', className: 'hint', textContent: 'The FM-1 is off.' });
    this.power = this.el('button', { id: 'emu-start', type: 'button', textContent: 'Start the FM-1' });
    this.power.addEventListener('click', () => this.start());
    this.meter = this.el('meter', { id: 'emu-level', min: 0, max: 1, value: 0, title: 'audio level' });
    const panel = this.el('div', { id: 'emu-panel', className: 'panel' });
    for (const [name, id] of BUTTONS) {
      const k = this.el('button', { type: 'button', className: 'key', textContent: name, title: name });
      k.dataset.key = String(id);
      let downAt = 0;                       // a click is held at least 100 ms: the firmware reads buttons a pass at a time
      k.addEventListener('pointerdown', () => { downAt = performance.now(); this.key(id, true); });
      for (const ev of ['pointerup', 'pointerleave']) {
        k.addEventListener(ev, () => {
          if (!downAt) return;
          const left = downAt + 100 - performance.now();
          downAt = 0;
          setTimeout(() => this.key(id, false), Math.max(0, left));
        });
      }
      panel.append(k);
    }
    const encs = this.el('div', { className: 'panel encs' });
    for (const [name, id] of ENCODERS) {
      const minus = this.el('button', { type: 'button', className: 'enc', textContent: '−', title: `${name} left` });
      const plus = this.el('button', { type: 'button', className: 'enc', textContent: '+', title: `${name} right` });
      minus.dataset.enc = plus.dataset.enc = String(id);
      minus.addEventListener('click', () => this.turn(id, -1));
      plus.addEventListener('click', () => this.turn(id, 1));
      encs.append(this.el('span', { className: 'encbox' }, minus, this.el('span', { textContent: name }), plus));
    }
    this.libList = this.el('ul', { id: 'emu-library' });
    b.append(this.el('div', { className: 'lcdwrap' }, this.canvas), this.el('div', { className: 'row' }, this.power, this.meter),
             this.status, panel, encs, this.el('h3', { textContent: 'On the FM-1' }), this.libList,
             this.el('p', { className: 'hint', textContent: 'Keys: space PLAY, Esc HOME, arrows SELECT, E EDIT, S SAVE, Z undo. FM-1 emulator: fm1-emu by Joakim Eriksson (MIT).' }));
    document.addEventListener('keydown', (e) => {
      if (this.state !== 'on' || e.target.closest('input, textarea, select, dialog') || e.metaKey || e.ctrlKey || e.altKey) return;
      const s = SHORTCUTS[e.key.length === 1 ? e.key.toLowerCase() : e.key];
      if (!s) return;
      e.preventDefault();
      if (s[0] === 'key') this.tap(s[1]); else this.turn(s[1], s[2]);
    });
  }

  say(t) { this.status.textContent = t; }

  async start() {
    if (this.state === 'booting' || this.state === 'on') return;
    this.state = 'booting';
    this.power.disabled = true;
    this.say('Starting the FM-1…');
    try {
      this.P = await import('./emu/persistence.js').catch(() => { throw new Error('the emulator is not built'); });
      const [wasm, fwsc] = await Promise.all(['emu/fm1_web.wasm', 'firmware/composer.fwsc'].map(async (u) => {
        const r = await fetch(u, { cache: 'no-store' });
        if (!r.ok) throw new Error(`${u} is missing (${r.status})`);
        return r.arrayBuffer();
      }));
      this.hash = await this.P.packageHash(new Uint8Array(fwsc));
      this.ctx = new AudioContext({ sampleRate: 44100, latencyHint: 'playback' });
      await this.ctx.audioWorklet.addModule('emu/worklet.js');
      this.node = new AudioWorkletNode(this.ctx, 'fm1', { numberOfInputs: 0, outputChannelCount: [2] });
      this.analyser = this.ctx.createAnalyser();
      this.analyser.fftSize = 2048;
      this.node.connect(this.analyser);
      this.analyser.connect(this.ctx.destination);
      this.node.port.onmessage = (e) => this.onMessage(e.data);
      const sectors = this.saved();
      this.node.port.postMessage({ type: 'boot', seq: 1, wasm, package: fwsc, mhz: MHZ, sectors: sectors?.buffer });
      await this.ctx.resume();
      this.meterLoop();
    } catch (e) {
      this.state = 'failed';
      this.power.disabled = false;
      this.say(`The FM-1 did not start: ${e.message || e}. (sh app/tools/get_web_emu.sh builds the emulator; the firmware comes from CI.)`);
    }
  }

  saved() {
    try {
      const s = JSON.parse(localStorage.getItem(STORE) || 'null');
      return s && s.hash === this.hash ? unb64(s.sectors) : null;
    } catch { return null; }
  }

  keep(flash) {                               // the library survives a reload (in this browser)
    try {
      localStorage.setItem(STORE, JSON.stringify({ hash: this.hash, sectors: b64(this.P.packSectors(this.P.changedSectors(this.base, flash))) }));
    } catch { /* full or private: the library lasts this visit only */ }
  }

  onMessage(m) {
    if (m.type === 'booted') {
      this.state = 'on';
      this.base = m.base;
      this.flash = m.flash;
      this.power.textContent = 'Running';
      this.say('COMPOSER is running on the FM-1.');
      this.showLibrary();
      document.dispatchEvent(new CustomEvent('studio:fm1', { detail: 'on' }));
    } else if (m.type === 'frame') {
      if (m.lcd) { this.lcd = m.lcd; this.draw(); }
      this.ms = m.ms;
    } else if (m.type === 'snapshot' && m.flash) {
      this.flash = m.flash;
      this.keep(m.flash);
      this.showLibrary();
    } else if (m.type === 'error') {
      if (m.fatal || this.state === 'booting') {      // it stopped: Start again
        this.state = 'failed';
        this.power.disabled = false;
        this.power.textContent = 'Start the FM-1';
      }
      this.say(`The emulator stopped: ${m.message}`);
    }
    const ready = this.waiters.filter((w) => w.test(m));
    this.waiters = this.waiters.filter((w) => !ready.includes(w));
    for (const w of ready) w.resolve(m);
  }

  wait(test, ms, what) {
    return new Promise((resolve, reject) => {
      const w = { test, resolve };
      this.waiters.push(w);
      setTimeout(() => { if (this.waiters.includes(w)) { this.waiters = this.waiters.filter((x) => x !== w); reject(new Error(what)); } }, ms);
    });
  }

  draw() {
    const img = new ImageData(240, 240), px = new Uint32Array(img.data.buffer);
    for (let i = 0; i < px.length; i++) px[i] = LUT[this.lcd[i]];
    this.canvas.getContext('2d').putImageData(img, 0, 0);
  }

  meterLoop() {
    const buf = new Float32Array(this.analyser.fftSize);
    const tick = () => {
      this.analyser.getFloatTimeDomainData(buf);
      let s = 0;
      for (const x of buf) s += x * x;
      this.level = Math.sqrt(s / buf.length);
      this.meter.value = Math.min(1, this.level * 4);
      this.meter.dataset.level = this.level.toFixed(5);
      setTimeout(tick, 100);
    };
    tick();
  }

  key(id, down) { if (this.state === 'on') this.node.port.postMessage({ type: 'key', id, down }); }
  tap(id) { this.key(id, true); setTimeout(() => this.key(id, false), 120); }
  turn(enc, clicks) { if (this.state === 'on') this.node.port.postMessage({ type: 'enc', enc, clicks }); }

  showLibrary() {
    const lib = library(this.flash);
    this.libList.replaceChildren(...(lib.length ? lib : [null]).map((p) => this.el('li', { textContent: p ? `${p.name} (${p.bytes.length} bytes)` : 'No pieces yet.' })));
    return lib;
  }

  /** push one piece; onProgress(0..1). Resolves when the piece is read back from the FM-1's flash. */
  async push(piece, name, onProgress = () => {}) {
    if (this.state !== 'on') throw new Error('Start the FM-1 first.');
    if (this.pushing) throw new Error('A push is already running.');
    this.pushing = true;
    try {
      const bytes = transferBytes(piece);
      let sent = 0;
      const t0 = this.ms;
      while (sent < bytes.length) {
        await this.wait((m) => m.type === 'frame', 5000, 'The FM-1 stopped running.');
        const allowed = Math.min(bytes.length, (this.ms - t0) * BYTES_PER_MS + 300);
        const n = Math.min(allowed - sent, 1500);
        if (n > 0) {
          this.node.port.postMessage({ type: 'midi', bytes: Array.from(bytes.subarray(sent, sent + n)) });
          sent += n;
          onProgress(sent / bytes.length);
        }
      }
      const done = this.ms + 1500;              // the jack's queue drains (<= 300 bytes ahead), the save runs
      while (this.ms < done) await this.wait((m) => m.type === 'frame', 5000, 'The FM-1 stopped running.');
      for (let i = 0; i < 6; i++) {
        const req = Math.random();
        this.node.port.postMessage({ type: 'snapshot', seq: 1, request: req });
        const s = await this.wait((m) => m.type === 'snapshot' && m.request === req, 4000, 'The FM-1\'s flash could not be read.');
        if (s.flash) {
          const hit = library(s.flash).find((p) => p.bytes.length === piece.length && p.bytes.every((x, k) => x === piece[k]));
          if (hit) return hit;
        }
        await new Promise((r) => setTimeout(r, 500));
      }
      throw new Error(`The FM-1 did not save "${name}" (if a piece is playing, it is saved when it stops). Try Push again.`);
    } finally {
      this.pushing = false;
    }
  }
}
