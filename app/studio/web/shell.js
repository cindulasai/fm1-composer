// COMPOSER Studio: text2midi's window (left) and fm1-emu's page running COMPOSER (right), side by side, joined by a
// MIDI cable. Send to FM-1 fits text2midi's open piece for the FM-1 (every change reported) and plays its transfer
// SysEx into the cable, paced to the MIDI jack's 31250 baud (slower while the emulator runs slower than real time).
import { transferFrames, savedLibraries } from './fm1link.js';

const $ = (s) => document.querySelector(s);
const BYTES_PER_MS = 2.5;                     // under the jack's 3.125 bytes/ms, with room for a slow emulation
const JACK_PER_MS = 3.125, AHEAD = 1200;     // the jack takes 3.125 bytes per emulated ms; fm1-emu drops past ~1 s of
                                              // backlog, so Send stays at most ~0.4 s ahead of its emulated time
const left = () => $('#t2m').contentWindow, right = () => $('#fm1').contentWindow;
let sending = false;

function say(text, kind = '') { $('#status').textContent = text; $('#status').className = kind; }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function openPiece() {                         // text2midi's open piece, or null
  const desk = left()?.studioDesk;
  if (!desk) return null;
  try { desk.call('view'); return desk; } catch { return null; }
}

setInterval(() => { if (!sending) $('#send').disabled = !openPiece(); }, 500);

$('#send').addEventListener('click', async () => {
  if (sending) return;
  const desk = openPiece();
  if (!desk) return say('Open or make a piece in text2midi first.', 'error');
  const cable = right()?.composerCable;
  if (!cable) return say('The FM-1 emulator has not loaded yet.', 'error');
  if (!cable.connected()) return say(`Press Start on the FM-1, and choose "${cable.name}" in its MIDI in.`, 'error');
  let made;
  try { made = desk.fitted(); } catch (e) { return say(String(e), 'error'); }
  const report = made.report ? made.report.split('\n') : [];
  $('#report ul').replaceChildren(...report.map((t) => Object.assign(document.createElement('li'), { textContent: t })));
  $('#report').hidden = !report.length;
  sending = true;
  $('#send').disabled = true;
  say('Checking the FM-1 …');
  let before = -1;                             // the newest piece in the FM-1's saved flash now: Send must add a newer one
  try { before = Math.max(-1, ...(await savedLibraries()).flat().map((p) => p.seq)); } catch { /* nothing saved yet */ }
  const slow = () => { try { return Math.max(1, Math.min(10, cable.load())); } catch { return 1; } };
  const bar = $('#send-progress');
  bar.hidden = false;
  try {
    const frames = transferFrames(made.f1p), total = frames.reduce((n, f) => n + f.length, 0);
    let sent = 0;
    say(`Sending "${made.name}": ${made.channels} channels, ${made.notes} notes, ${made.bars} bars …`);
    const emuMs = () => { try { return cable.emulatedMs(); } catch { return NaN; } };
    const e0 = emuMs();
    let waits = 0;                             // (50 ms each; 30 s in all, then on as before: no hang)
    for (const f of frames) {
      // a stalled emulator (a busy computer) takes nothing: wait for it rather than overflow its jack
      while (Number.isFinite(e0) && waits < 600 && sent + f.length - (emuMs() - e0) * JACK_PER_MS > AHEAD) { waits++; await sleep(50); }
      cable.send(f);
      sent += f.length;
      bar.value = sent / total;
      await sleep(f.length * slow() / BYTES_PER_MS);   // (fm1-emu drops what is more than a second behind)
    }
    const same = (p) => p.seq > before && p.bytes.length === made.f1p.length && p.bytes.every((x, k) => x === made.f1p[k]);
    let saved = false;
    const eSent = emuMs(), t1 = performance.now();
    const log = window.studioSendLog = { e0, eSent, waits, bytes: sent, reads: 0 };   // (what a failed Send leaves to look at)
    const more = Number.isFinite(eSent)        // read back from fm1-emu's saved flash: until it has run 10 s of its own time
      ? () => emuMs() - eSent < 10000 && performance.now() - t1 < 90000     // (90 s at most: a computer too busy to run it)
      : ((tries) => () => log.reads < tries)(Math.ceil(20 * slow()));      // (no emulated time shown: 10 s, longer on a slow one)
    while (!saved && more()) {
      await sleep(500);
      log.reads++;
      try { saved = (await savedLibraries()).some((lib) => lib.some(same)); } catch { /* not saved yet */ }
    }
    log.eEnd = emuMs();
    if (!saved) throw new Error(`the FM-1 did not save "${made.name}" (if a piece is playing, it is saved when it stops; then Send again)`);
    say(`On the FM-1: "${made.name}" (read back from its flash; ${made.channels} channels, ${made.notes} notes, ${made.bars} bars, ${report.length ? `${report.length} changes to fit` : 'nothing had to change'}). It is at the top of LIBRARY: press PLAY.`);
  } catch (e) {
    say(`Sending stopped: ${e.message || e}`, 'error');
  } finally {
    sending = false;
    bar.hidden = true;
    $('#send').disabled = !openPiece();
  }
});
