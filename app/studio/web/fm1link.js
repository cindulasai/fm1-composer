// COMPOSER's transfer SysEx (as app/tools/fm1c/link.py writes it): PUT, the 256-byte chunks, COMMIT. And the FM-1's
// footer read off its screen (US-34: a piece sent while it plays shows NEXT: <name>, then LIVE: <name>).
import { FONT_S } from './font.js';
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

/** the same transfer as whole SysEx messages, one per frame (as a MIDI cable carries them) */
export function transferFrames(piece) {
  const out = [Uint8Array.from(frame(3, u32(piece.length)))];
  for (let off = 0; off < piece.length; off += 256) {
    const c = new Uint8Array(4 + Math.min(256, piece.length - off));
    c.set(u32(off));
    c.set(piece.subarray(off, off + 256), 4);
    out.push(Uint8Array.from(frame(4, c)));
  }
  out.push(Uint8Array.from(frame(5)));
  return out;
}

const LIB_AT = 0x97000, LIB_SECT = 73, LIB_MAGIC = 0x3142494c;   // COMPOSER's library (firmware/src/store.h, lib.c)
const FLASH = 1 << 20, SECTOR = 4096;

/** COMPOSER's library in a flash image: [{ seq, name, bytes }], newest first */
export function library(flash) {
  const v = new DataView(flash.buffer, flash.byteOffset, flash.byteLength), out = [];
  for (let s = 0; s < LIB_SECT; s++) {
    const o = LIB_AT + s * SECTOR;
    if (v.getUint32(o, true) !== LIB_MAGIC || v.getUint32(o + 48, true) !== 0xffffffff) continue;
    const len = v.getUint32(o + 16, true);
    if (len < 1 || len > 32768 || o + 64 + len > flash.length) continue;
    const bytes = flash.slice(o + 64, o + 64 + len);
    if (crc32(bytes) !== v.getUint32(o + 20, true)) continue;
    const raw = flash.subarray(o + 24, o + 48);
    const name = String.fromCharCode(...raw.subarray(0, raw.indexOf(0) < 0 ? 24 : raw.indexOf(0)));
    out.push({ seq: v.getUint32(o + 12, true), name, bytes });
  }
  return out.sort((a, b) => b.seq - a.seq);
}

/** the libraries in fm1-emu's saved flash (its IndexedDB "fm1-emulator", one state per firmware) */
export async function savedLibraries(idb = indexedDB) {
  const db = await new Promise((ok, no) => { const r = idb.open('fm1-emulator'); r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
  try {
    if (!db.objectStoreNames.contains('states')) return [];
    const states = await new Promise((ok, no) => {
      const r = db.transaction('states', 'readonly').objectStore('states').getAll();
      r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error);
    });
    return states.map((st) => {
      const flash = new Uint8Array(FLASH).fill(0xff);
      for (const s of st.sectors || []) if (s.at + SECTOR <= FLASH) flash.set(new Uint8Array(s.data), s.at);
      return library(flash);
    });
  } finally { db.close(); }
}

// ---- the FM-1's footer (firmware/src/gfx.c draw_text_box: FONT_S centred in 240 x 16 at row 222, C_GREY on black) ----
const GLYPHS = Uint8Array.from(atob(FONT_S.data), (c) => c.charCodeAt(0));
const GREY = (120 >> 3) << 11 | (120 >> 2) << 5 | (120 >> 3), FOOT_Y = 222;

/** the name the FM-1 shows for a piece: its 24 name bytes up to the first 0, other than 32..126 as '?' (composer.c) */
export function screenName(piece) {
  const raw = piece.subarray(16, 40), end = raw.indexOf(0) < 0 ? 24 : raw.indexOf(0);
  return String.fromCharCode(...[...raw.subarray(0, end)].map((c) => (c >= 32 && c <= 126 ? c : 63)));
}

/** the footer's pixels (RGB565, 240 x 16) as the FM-1 draws this text */
export function footerPixels(text) {
  const f = FONT_S, w = 240, out = new Uint16Array(w * f.h);
  const r = GREY >> 11, g = (GREY >> 5) & 63, b = GREY & 31;
  const ramp = Array.from({ length: 16 }, (_, a) => (Math.floor(r * a / 15) << 11) | (Math.floor(g * a / 15) << 5) | Math.floor(b * a / 15));
  const ks = [...text].map((ch) => { const c = ch.charCodeAt(0); return (c < f.first || c > f.last ? 63 : c) - f.first; });
  let x = Math.trunc((w - ks.reduce((n, k) => n + f.adv[k], 0)) / 2);
  for (const k of ks) {
    const bw = f.bw[k], bpr = (bw + 1) >> 1;
    for (let y = 0; y < f.h; y++) {
      for (let gx = 0; gx < bw; gx++) {
        const byte = GLYPHS[f.off[k] + y * bpr + (gx >> 1)], v = gx & 1 ? byte & 15 : byte >> 4, px = x - f.pad + gx;
        if (v && px >= 0 && px < w) out[y * w + px] = ramp[v];
      }
    }
    x += f.adv[k];
  }
  return out;
}

/** the FM-1's screen (RGB565, 240 x 240) shows exactly this footer */
export function footerSays(lcd, text) {
  if (!lcd || lcd.length !== 240 * 240) return false;
  const want = footerPixels(text);
  for (let i = 0; i < want.length; i++) if (lcd[FOOT_Y * 240 + i] !== want[i]) return false;
  return true;
}
