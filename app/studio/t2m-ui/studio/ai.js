// The language model, asked from the browser with the user's key: what text2midi's app/src-tauri/src/ai.rs does
// with genai, for the providers that answer a web page. Settings are kept in this browser; a key from
// local/ai.json (git-ignored, tools/studio_key.sh) is used when none is saved here.
const params = new URLSearchParams(location.search);
export const TIMEOUT_MS = Number(params.get('timeout')) || 120000;          // text2midi's REQUEST_SECONDS
const P = (id, label, key_url, models, base, extra = {}) =>
  ({ id, label, key_url, needs_key: true, default_url: null, needs_url: false, models, base, kind: 'openai', ...extra });
// text2midi's ai::providers(), in its order, with each one's OpenAI-compatible address
export const PROVIDERS = [
  P('anthropic', 'Anthropic (Claude)', 'https://console.anthropic.com/settings/keys', ['claude-sonnet-4-5', 'claude-haiku-4-5', 'claude-opus-4-1'], 'https://api.anthropic.com/v1', { kind: 'anthropic' }),
  P('openai', 'OpenAI', 'https://platform.openai.com/api-keys', ['gpt-4o-mini', 'gpt-4o', 'gpt-4.1-mini'], 'https://api.openai.com/v1'),
  P('gemini', 'Google Gemini', 'https://aistudio.google.com/apikey', ['gemini-2.5-flash', 'gemini-2.5-pro'], 'https://generativelanguage.googleapis.com/v1beta/openai'),
  P('deepseek', 'DeepSeek', 'https://platform.deepseek.com/api_keys', ['deepseek-chat', 'deepseek-reasoner'], 'https://api.deepseek.com'),
  P('groq', 'Groq', 'https://console.groq.com/keys', ['llama-3.3-70b-versatile'], 'https://api.groq.com/openai/v1'),
  P('xai', 'xAI (Grok)', 'https://console.x.ai', ['grok-3-mini', 'grok-3'], 'https://api.x.ai/v1'),
  P('mistral', 'Mistral', 'https://console.mistral.ai/api-keys', ['mistral-small-latest', 'mistral-large-latest'], 'https://api.mistral.ai/v1'),
  P('openrouter', 'OpenRouter (many providers)', 'https://openrouter.ai/keys', ['anthropic/claude-sonnet-4.5', 'openai/gpt-4o-mini', 'deepseek/deepseek-chat'], 'https://openrouter.ai/api/v1'),
  P('minimax', 'MiniMax', 'https://www.minimax.io', ['MiniMax-M3.1-Flash', 'MiniMax-M3'], 'https://api.minimax.io/v1', { default_url: 'https://api.minimax.io/v1/' }),
  P('ollama', 'Ollama (runs on this computer, no key)', 'https://ollama.com', ['llama3.2', 'qwen2.5', 'mistral'], 'http://localhost:11434/v1', { needs_key: false }),
  P('custom', 'Other (OpenAI-compatible)', '', [], '', { needs_url: true }),
];
const byId = (id) => PROVIDERS.find((p) => p.id === id);
const STORE = 'composer-studio.t2m-ai';
const read = () => { try { return JSON.parse(localStorage.getItem(STORE) || '{}') || {}; } catch { return {}; } };
const write = (v) => { try { localStorage.setItem(STORE, JSON.stringify(v)); } catch { /* private mode */ } };
let local = null;
export async function loadLocal() {
  try {
    const r = await fetch(new URL('../local/ai.json', location.href), { cache: 'no-store' });
    if (r.ok) { const j = await r.json(); if (j && typeof j.key === 'string' && j.key) local = j; }
  } catch { /* none: Settings is the way */ }
}
// the model "Make it" uses: Settings here (with its key), else the key in local/ai.json, else none (offline reader)
export function chosen() {
  const s = read();
  if (s.settings && (s.keys?.[s.settings.provider] || !byId(s.settings.provider)?.needs_key))
    return { ...s.settings, key: s.keys?.[s.settings.provider] || '' };
  if (local) return { provider: local.provider || 'minimax', model: local.model || 'MiniMax-M3.1-Flash', base_url: local.base || null, key: local.key };
  return null;
}
export const describe = (m) => `${byId(m.provider)?.label ?? '?'} · ${m.model}`;
const publicProviders = () => PROVIDERS.map(({ base, kind, ...p }) => p);
export function info() {
  const s = read(), m = chosen();
  return { settings: s.settings ?? (local ? { provider: local.provider || 'minimax', model: local.model || 'MiniMax-M3.1-Flash', base_url: null } : null),
           has_key: !!(s.settings && s.keys?.[s.settings.provider]) || (!s.settings && !!local),
           active: m ? describe(m) : null, providers: publicProviders() };
}
export function save(settings, key) {
  if (!byId(settings.provider)) throw 'unknown provider';
  if (!String(settings.model || '').trim()) throw 'choose a model';
  const s = read();
  s.settings = { provider: settings.provider, model: settings.model.trim(), base_url: settings.base_url || null };
  if (key && key.trim()) {
    if (/[\s\u0000-\u001f]/.test(key.trim())) throw 'the key has spaces or control characters in it';
    s.keys = { ...(s.keys || {}), [settings.provider]: key.trim() };
  }
  write(s);
  return info();
}
export function removeKey(provider) {
  if (!byId(provider)) throw 'unknown provider';
  const s = read();
  if (s.keys) delete s.keys[provider];
  write(s);
  return info();
}
function errorText(body) {
  try { const j = JSON.parse(body); return String(j.error?.message || j.base_resp?.status_msg || body).slice(0, 200); }
  catch { return String(body).slice(0, 200); }
}
// one request: system + user → the model's text (throws the reason)
export async function complete(m, system, user, replyText) {
  const p = byId(m.provider) || byId('custom');
  const base = String(m.base_url || p.base || '').replace(/\/+$/, '');
  if (!base) throw 'no address set for this provider';
  const headers = { 'content-type': 'application/json' };
  let url, body;
  if (p.kind === 'anthropic') {
    url = `${base}/messages`;
    Object.assign(headers, { 'x-api-key': m.key, 'anthropic-version': '2023-06-01', 'anthropic-dangerous-direct-browser-access': 'true' });
    body = { model: m.model, max_tokens: 8000, system, messages: [{ role: 'user', content: user }] };
  } else {
    url = `${base}/chat/completions`;
    if (m.key) headers.authorization = `Bearer ${m.key}`;
    body = { model: m.model, messages: [{ role: 'system', content: system }, { role: 'user', content: user }] };
    if (m.provider === 'minimax') Object.assign(body, { max_completion_tokens: 8000, reasoning_split: true });
    else body.max_tokens = 8000;
  }
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), TIMEOUT_MS);
  let res, text;
  try {
    res = await fetch(url, { method: 'POST', headers, body: JSON.stringify(body), signal: ctl.signal });
    text = await res.text();
  } catch {
    throw ctl.signal.aborted ? `no answer within ${TIMEOUT_MS / 1000} s` : `could not reach ${p.label}`;
  } finally { clearTimeout(timer); }
  if (res.status === 401 || res.status === 403) throw `${p.label} refused the key (${res.status})`;
  if (!res.ok) throw `${p.label} answered ${res.status}: ${errorText(text)}`;
  if (p.kind === 'anthropic') {
    let j;
    try { j = JSON.parse(text); } catch { throw 'the reply was not JSON'; }
    const t = (j.content || []).filter((c) => c.type === 'text').map((c) => c.text).join('');
    if (!t.trim()) throw 'the reply had no text';
    return t;
  }
  try { return replyText(text); } catch (e) { throw String(e); }
}
