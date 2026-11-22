// Stand-in for @tauri-apps/api/core in the browser: text2midi's window calls invoke(command, args) exactly as in its
// desktop app; each command is what app/src-tauri/src/main.rs does, here with t2m-session and t2m-audio compiled to
// WebAssembly (studio/engine, desk.rs), the language model asked from the page (ai.js), projects kept in this browser.
import { emit } from './event.js';
import * as ai from './ai.js';

const engineUrl = new URL('../engine/studio.js', location.href).href;
let ready = null, E = null, desk = null, ctx = null, node = null, level = 0;
const PROJECTS = 'composer-studio.t2m-projects';
const MAX_TEXT_CHARS = 2000;                                                 // t2m_intent::MAX_TEXT_CHARS

function boot() {
  ready ||= (async () => {
    E = await import(/* @vite-ignore */ engineUrl);
    await E.default();
    ctx = new AudioContext();
    ctx.suspend();
    desk = new E.Desk(ctx.sampleRate);
    window.studioDesk = { fitted: () => desk.fitted(), call: (c, a) => JSON.parse(desk.call(c, JSON.stringify(a ?? {}))),
                          level: () => level };
    await ai.loadLocal();
  })();
  return ready;
}
const call = (cmd, args) => JSON.parse(desk.call(cmd, JSON.stringify(args ?? {})));

function sound() {                                       // the preview: the engine renders into the page's audio
  if (!node) {
    node = ctx.createScriptProcessor(2048, 0, 2);
    const buf = new Float32Array(4096);
    node.onaudioprocess = (e) => {
      const l = e.outputBuffer.getChannelData(0), r = e.outputBuffer.getChannelData(1);
      desk.render(buf);
      let peak = 0;
      for (let i = 0; i < l.length; i++) { l[i] = buf[2 * i]; r[i] = buf[2 * i + 1]; peak = Math.max(peak, Math.abs(l[i]), Math.abs(r[i])); }
      level = peak;
    };
    node.connect(ctx.destination);
  }
  return ctx.resume();
}

// text2midi's brief_from_prompt: ask, read the brief; one more try with what was wrong
async function cloudBrief(text, report) {
  const m = ai.chosen();
  if (!m) return { brief: null, why: null };
  const system = E.prompt();
  let attempt = 0;
  const ask = (user) => { report?.({ stage: 'asking', attempt: ++attempt }); return ai.complete(m, system, user, E.reply_text); };
  try {
    const first = await ask(text.trim());
    try { return { brief: E.brief_from_reply(first), model: ai.describe(m) }; } catch (problem) {
      const second = await ask(`${text.trim()}\n\nYour previous answer could not be used (${problem}). Answer again with only the JSON brief.`);
      try { return { brief: E.brief_from_reply(second), model: ai.describe(m) }; } catch (p2) { return { brief: null, why: `the answer could not be used: ${p2}` }; }
    }
  } catch (e) { return { brief: null, why: String(e) }; }
}

const projects = () => { try { return JSON.parse(localStorage.getItem(PROJECTS) || '{}') || {}; } catch { return {}; } };
function putProject(name, json) {
  const all = projects();
  all[name] = { json, time: Date.now() };
  try { localStorage.setItem(PROJECTS, JSON.stringify(all)); } catch { throw 'this browser has no room left to keep the project'; }
}
// main.rs project_file_name
function fileName(title, id, seed) {
  const slug = (t) => {
    let out = '';
    for (const c of t) {
      if ((/[A-Za-z0-9]/.test(c)) || c === '_') out += c.toLowerCase();
      else if (!out.endsWith('-') && out) out += '-';
      if (out.length >= 40) break;
    }
    return out.replace(/^-+|-+$/g, '');
  };
  return `${slug(title) || slug(id) || 'piece'}-${seed}.t2m.json`;
}
function unused(name) {
  const all = projects(), stem = name.replace(/\.t2m\.json$/, '');
  if (!all[name]) return name;
  for (let n = 2; n < 10000; n++) if (!all[`${stem}-${n}.t2m.json`]) return `${stem}-${n}.t2m.json`;
  return `${stem}-${Date.now()}.t2m.json`;
}
const validName = (n) => typeof n === 'string' && n && !/[/\\\0]/.test(n) && !n.startsWith('.') && n.endsWith('.t2m.json');

const commands = {
  gallery: () => call('gallery'),
  open_example: (a) => call('open_example', a),
  async open_prompt({ text, seed }) {
    if (!text.trim() || [...text].length > MAX_TEXT_CHARS) return call('open_prompt', { text, seed });
    const got = await cloudBrief(text);
    return call('open_prompt', { text, seed, brief: got.brief ?? undefined, asked: true, ai_error: got.why ?? undefined });
  },
  async make_piece({ text, seed }) {
    const got = await cloudBrief(text, (s) => emit('compose-progress', s));
    emit('compose-progress', { stage: 'composing' });
    const made = call('make', { text, seed, brief: got.brief ?? undefined, ai_error: got.why ?? undefined });
    emit('compose-progress', { stage: 'saving' });
    const name = unused(fileName(made.title, made.gallery_id, made.seed));
    putProject(name, made.json);
    return { name, title: made.title, source: made.source, note: made.note, model: made.source === 'cloud' ? got.model : null };
  },
  ai_settings: () => ai.info(),
  save_ai_settings: ({ settings, key }) => ai.save(settings, key),
  remove_ai_key: ({ provider }) => ai.removeKey(provider),
  async test_ai() {
    const m = ai.chosen();
    if (!m) throw 'Choose a provider and model, and save a key, first.';
    const said = (await ai.complete(m, 'Answer with the single word OK.', 'Are you there?', E.reply_text)).trim().slice(0, 40);
    return `${ai.describe(m)} answered: "${said}"`;
  },
  takes: (a) => call('takes', a), keep_take: (a) => call('keep_take', a), reroll: (a) => call('reroll', a),
  reseed: (a) => call('reseed', a), lock: (a) => call('lock', a), undo: () => call('undo'), redo: () => call('redo'),
  mix: (a) => call('mix', a),
  async play(a) { await sound(); return call('play', a); },
  stop: () => call('stop'), transport: () => call('transport'),
  export_midi({ profile }) {
    const files = desk.export(profile), out = [];
    for (let i = 0; i < files.count(); i++) {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([files.bytes(i)], { type: 'audio/midi' }));
      a.download = files.name(i);
      a.click();
      out.push(`Downloads/${files.name(i)}`);
    }
    return out;
  },
  save_project() {
    const s = call('save'), name = fileName(s.title, s.gallery_id, s.seed);
    putProject(name, s.json);
    return name;
  },
  saved_projects: () => Object.entries(projects()).sort((a, b) => b[1].time - a[1].time).map(([n]) => n),
  open_project({ name }) {
    if (!validName(name)) throw 'that is not a saved project name';
    const p = projects()[name];
    if (!p) throw `cannot read ${name}: not found`;
    return call('open_project', { json: p.json });
  },
};

export async function invoke(cmd, args) {
  window.studioInvoke ||= invoke;
  await boot();
  const f = commands[cmd];
  if (!f) throw `unknown command ${cmd}`;
  try { return await f(args ?? {}); } catch (e) { throw typeof e === 'string' ? e : (e?.message ?? String(e)); }
}
