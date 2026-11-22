// COMPOSER Studio: an idea → the AI (called from this page) → a brief → text2midi (WASM) → a piece for the FM-1.
import init, * as E from './engine/studio.js';
import { Fm1 } from './emu.js';

const $ = (s) => document.querySelector(s);
const params = new URLSearchParams(location.search);
const TIMEOUT_MS = Number(params.get('timeout')) || 120000;   // as text2midi's app: a full brief can take a minute
const MAX_IDEA = 2000;                                  // as text2midi's intent crate
const PROVIDERS = {
  minimax: { label: 'MiniMax', base: 'https://api.minimax.io/v1', model: 'MiniMax-M3.1-Flash', kind: 'openai' },
  deepseek: { label: 'DeepSeek', base: 'https://api.deepseek.com', model: 'deepseek-chat', kind: 'openai' },
  anthropic: { label: 'Anthropic', base: 'https://api.anthropic.com/v1', model: 'claude-haiku-5-5', kind: 'anthropic' },
  openai: { label: 'OpenAI-compatible', base: 'https://api.openai.com/v1', model: '', kind: 'openai' },
};
const STORE = 'composer-studio.ai';
const state = { brief: null, made: null, busy: false, local: null, pushing: false, fm1: null };

function say(text, kind = '') {
  const s = $('#status');
  s.textContent = text;
  s.className = kind;
}

function readStore() {
  try { return JSON.parse(localStorage.getItem(STORE) || 'null'); } catch { return null; }
}

async function aiConfig() {
  const saved = readStore();
  if (saved && saved.key) return saved;
  return state.local || {};
}

async function loadLocal() {                            // local/ai.json: git-ignored, written by tools/studio_key.sh
  try {
    const r = await fetch('local/ai.json', { cache: 'no-store' });
    if (r.ok) { const j = await r.json(); if (j && typeof j.key === 'string' && j.key) state.local = j; }
  } catch { /* none: the settings dialog is the way */ }
}

function errorText(body) {
  try { const j = JSON.parse(body); return String(j.error?.message || j.base_resp?.status_msg || body).slice(0, 200); }
  catch { return String(body).slice(0, 200); }
}

async function ask(cfg, system, user) {
  const p = PROVIDERS[cfg.provider] || PROVIDERS.minimax;
  const base = (cfg.base || p.base).replace(/\/+$/, '');
  const model = cfg.model || p.model;
  if (!model) throw new Error('Choose a model in Settings.');
  const headers = { 'content-type': 'application/json' };
  let url, body;
  if (p.kind === 'anthropic') {
    url = `${base}/messages`;
    Object.assign(headers, { 'x-api-key': cfg.key, 'anthropic-version': '2023-06-01', 'anthropic-dangerous-direct-browser-access': 'true' });
    body = { model, max_tokens: 8000, system, messages: [{ role: 'user', content: user }] };
  } else {
    url = `${base}/chat/completions`;
    headers.authorization = `Bearer ${cfg.key}`;
    body = { model, messages: [{ role: 'system', content: system }, { role: 'user', content: user }] };
    if ((cfg.provider || 'minimax') === 'minimax') Object.assign(body, { max_completion_tokens: 8000, reasoning_split: true });
    else if (cfg.provider === 'deepseek') body.max_tokens = 8000;
    else body.max_completion_tokens = 8000;
  }
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), TIMEOUT_MS);
  const started = Date.now(), waiting = $('#status').textContent;
  const ticker = setInterval(() => say(`${waiting} ${Math.round((Date.now() - started) / 1000)} s`), 1000);
  let res, text;
  try {
    res = await fetch(url, { method: 'POST', headers, body: JSON.stringify(body), signal: ctl.signal });
    text = await res.text();
  } catch {
    throw new Error(ctl.signal.aborted ? `The AI took longer than ${TIMEOUT_MS / 1000} s to answer. Try again.`
                                       : `Could not reach the AI (${p.label}). Check the connection and try again.`);
  } finally {
    clearTimeout(timer);
    clearInterval(ticker);
  }
  if (res.status === 401 || res.status === 403) throw new Error(`The AI refused the key (${res.status}). Check it in Settings.`);
  if (!res.ok) throw new Error(`The AI answered ${res.status}: ${errorText(text)}`);
  if (p.kind === 'anthropic') {
    let j;
    try { j = JSON.parse(text); } catch { throw new Error('The AI\'s reply was not JSON.'); }
    const t = (j.content || []).filter((c) => c.type === 'text').map((c) => c.text).join('');
    if (!t.trim()) throw new Error('The AI\'s reply had no text.');
    return t;
  }
  try { return E.reply_text(text); } catch (e) { throw new Error(String(e)); }
}

function seedValue() {
  const v = $('#seed').value.trim();
  if (/^\d{1,9}$/.test(v)) return Number(v);
  const s = 1 + Math.floor(Math.random() * 999999999);
  $('#seed').value = String(s);
  return s;
}

function li(list, items, empty) {
  list.replaceChildren(...(items.length ? items : [empty]).map((t) => { const e = document.createElement('li'); e.textContent = t; return e; }));
}

function showBrief(json) {
  const b = JSON.parse(json);
  const id = b.identity || {}, time = b.time || {}, ton = b.tonality || {};
  $('#brief-title').textContent = id.title || 'Untitled';
  $('#brief-style').textContent = id.style || '';
  $('#brief-tempo').textContent = time.tempo_bpm ? `${time.tempo_bpm} BPM` : '';
  $('#brief-key').textContent = [ton.tonic, (ton.system || '').replace(/^mode\./, '')].filter(Boolean).join(' ');
  const bars = (b.form || []).reduce((n, s) => n + (s.bars || 0), 0);
  $('#brief-length').textContent = `${bars} bars, ${(b.form || []).length} sections`;
  $('#brief-tracks').textContent = (b.tracks || []).map((t) => `${t.role} (${t.instrument})`).join(', ');
  li($('#brief-assumptions'), (b.assumptions || []).map((a) => `${a.field}: ${a.because}`), 'Nothing: the idea said it all.');
  $('#brief').hidden = false;
}

function showPiece(m) {
  state.made = m;
  $('#piece-name').textContent = m.name;
  $('#piece-stats').textContent = `${m.channels} channels, ${m.notes} notes, ${m.bars} bars, ${m.bpm} BPM, ${m.f1p.length} bytes`;
  li($('#piece-report'), m.report ? m.report.split('\n') : [], 'Nothing had to change.');
  $('#piece').hidden = false;
  $('#push').disabled = state.pushing;
  $('#push-progress').hidden = true;
  document.dispatchEvent(new CustomEvent('studio:piece', { detail: m }));
}

function compose(json) {
  const seed = seedValue();
  const name = JSON.parse(json).identity?.title || 'piece';
  try {
    showPiece(E.make(json, seed, name));
    say(`Composed with seed ${seed}.`);
    $('#again').disabled = false;
  } catch (e) {
    say(String(e), 'error');
  }
}

async function generate() {
  if (state.busy) return;
  const idea = $('#idea').value.trim();
  if (!idea) return say('Describe the music first, in a few words.', 'error');
  const n = [...idea].length;
  if (n > MAX_IDEA) return say(`Keep the idea under ${MAX_IDEA} characters (it has ${n}).`, 'error');
  const cfg = await aiConfig();
  if (!cfg.key) { say('Add your AI key in Settings first.', 'error'); openSettings(); return; }
  state.busy = true;
  $('#generate').disabled = true;
  try {
    say('Asking the AI for a brief…');
    const system = E.prompt();
    const first = await ask(cfg, system, idea);
    let brief;
    try {
      brief = E.brief_from_reply(first);
    } catch (problem) {
      say('The first answer was not a usable brief; asking once more…');
      const again = `${idea}\n\nYour previous answer could not be used (${problem}). Answer again with only the JSON brief.`;
      const second = await ask(cfg, system, again);
      try { brief = E.brief_from_reply(second); } catch (p2) { throw new Error(`The AI's answer could not be used: ${p2}`); }
    }
    state.brief = brief;
    showBrief(brief);
    compose(brief);
  } catch (e) {
    say(e.message || String(e), 'error');
  } finally {
    state.busy = false;
    $('#generate').disabled = false;
  }
}

function download(bytes, name, type) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([bytes], { type }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

async function openMidi(file) {
  if (!file) return;
  if (file.size > 8 * 1024 * 1024) return say('That file is larger than the 8 MB Studio reads.', 'error');
  try {
    const m = E.convert(new Uint8Array(await file.arrayBuffer()), file.name.replace(/\.midi?$/i, ''));
    $('#brief').hidden = true;
    $('#again').disabled = true;
    state.brief = null;
    showPiece(m);
    say(`${file.name} fitted to the FM-1.`);
  } catch (e) {
    say(`${file.name}: ${e}`, 'error');
  }
}

async function pushPiece() {
  const m = state.made;
  if (!m || state.pushing) return;
  state.pushing = true;
  $('#push').disabled = true;
  const bar = $('#push-progress');
  const show = (p) => bar.setAttribute('value', String(Math.round(p * 100)));
  bar.hidden = false;
  show(0);
  try {
    say(`Pushing "${m.name}" to the FM-1…`);
    const hit = await state.fm1.push(m.f1p, m.name, show);
    show(1);
    say(`"${hit.name}" is on the FM-1: pick it in LIBRARY with SELECT, then PLAY.`);
  } catch (e) {
    bar.hidden = true;
    say(e.message || String(e), 'error');
  } finally {
    state.pushing = false;
    $('#push').disabled = false;
  }
}

function openSettings() {
  const cfg = readStore() || {};
  const p = cfg.provider || state.local?.provider || 'minimax';
  $('#ai-provider').value = PROVIDERS[p] ? p : 'minimax';
  $('#ai-key').value = cfg.key || '';
  $('#ai-model').value = cfg.model || '';
  $('#ai-base').value = cfg.base || '';
  fillPlaceholders();
  $('#settings-source').textContent = state.local && !cfg.key ? 'Using the key in local/ai.json (git-ignored). A key saved here takes its place.' : '';
  $('#settings').showModal();
}

function fillPlaceholders() {
  const p = PROVIDERS[$('#ai-provider').value];
  $('#ai-model').placeholder = p.model || 'model name';
  $('#ai-base').placeholder = p.base;
}

function saveSettings() {
  const cfg = { provider: $('#ai-provider').value, key: $('#ai-key').value.trim(), model: $('#ai-model').value.trim(), base: $('#ai-base').value.trim() };
  try {
    if (cfg.key) localStorage.setItem(STORE, JSON.stringify(cfg)); else localStorage.removeItem(STORE);
  } catch { say('This browser would not keep the settings (private window?).', 'error'); }
}

async function main() {
  $('#generate').addEventListener('click', generate);
  $('#idea').addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) generate(); });
  $('#again').addEventListener('click', () => { if (state.brief) { $('#seed').value = ''; compose(state.brief); } });
  $('#dl-f1p').addEventListener('click', () => state.made && download(state.made.f1p, `${state.made.name}.f1p`, 'application/octet-stream'));
  $('#dl-mid').addEventListener('click', () => state.made && download(state.made.midi, `${state.made.name}.mid`, 'audio/midi'));
  $('#midi-file').addEventListener('change', (e) => { openMidi(e.target.files[0]); e.target.value = ''; });
  $('#settings-open').addEventListener('click', openSettings);
  $('#push').addEventListener('click', pushPiece);
  state.fm1 = window.studioFm1 = new Fm1($('#emu-box'));
  $('#ai-provider').addEventListener('change', fillPlaceholders);
  $('#settings').addEventListener('close', () => { if ($('#settings').returnValue === 'save') saveSettings(); });
  await loadLocal();
  try {
    await init();
    $('#engine').textContent = 'Engine ready';
    $('#engine').classList.add('ok');
    $('#generate').disabled = false;
    document.body.dataset.ready = '1';
  } catch (e) {
    $('#engine').textContent = 'Engine failed to load';
    say(`The engine did not load (${e}). Build it: sh app/studio/build.sh`, 'error');
  }
}

main();
