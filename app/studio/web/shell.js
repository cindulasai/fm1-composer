// COMPOSER Studio: text2midi's window (left) and fm1-emu's page running COMPOSER (right), side by side, joined by a
// MIDI cable. Send to FM-1 fits text2midi's open piece for the FM-1 (every change reported) and plays its transfer
// SysEx into the cable, paced to the MIDI jack's 31250 baud.
import { transferFrames, savedLibraries } from './fm1link.js';

const $ = (s) => document.querySelector(s);
const BYTES_PER_MS = 2.5;                     // under the jack's 3.125 bytes/ms, with room for a slow emulation
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
  const bar = $('#send-progress');
  bar.hidden = false;
  try {
    const frames = transferFrames(made.f1p), total = frames.reduce((n, f) => n + f.length, 0);
    let sent = 0;
    say(`Sending "${made.name}": ${made.channels} channels, ${made.notes} notes, ${made.bars} bars …`);
    for (const f of frames) {
      cable.send(f);
      sent += f.length;
      bar.value = sent / total;
      await sleep(f.length / BYTES_PER_MS);
    }
    const same = (p) => p.bytes.length === made.f1p.length && p.bytes.every((x, k) => x === made.f1p[k]);
    let saved = false;
    for (let i = 0; i < 20 && !saved; i++) {    // read back from fm1-emu's saved flash
      await sleep(500);
      try { saved = (await savedLibraries()).some((lib) => lib.some(same)); } catch { /* not saved yet */ }
    }
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
