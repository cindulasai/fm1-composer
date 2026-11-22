// COMPOSER's transfer SysEx (as app/tools/fm1c/link.py writes it): PUT, the 256-byte chunks, COMMIT.
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
