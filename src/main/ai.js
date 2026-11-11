'use strict';
/*
 * Mindloom — AI assistant, bring-your-own-key.
 * Runs in the main process so API keys never reach the page. Keys are encrypted with the OS keychain
 * (Electron safeStorage) and each key is only ever sent to its own provider's address.
 *
 * Providers: Anthropic (official SDK) and any OpenAI-compatible chat API — OpenAI, Google Gemini, Mistral,
 * Groq, DeepSeek, xAI, OpenRouter, Together, local Ollama / LM Studio, or a custom endpoint.
 */
const fs = require('node:fs');
const path = require('node:path');

const PROVIDERS = [
  { id: 'anthropic', name: 'Anthropic (Claude)', kind: 'anthropic', group: 'Cloud', keyUrl: 'https://console.anthropic.com/settings/keys', keyHint: 'sk-ant-…', defaultModel: 'claude-opus-5-5' },
  { id: 'openai', name: 'OpenAI (ChatGPT)', kind: 'openai', group: 'Cloud', base: 'https://api.openai.com/v1', keyUrl: 'https://platform.openai.com/api-keys', keyHint: 'sk-…' },
  { id: 'google', name: 'Google Gemini', kind: 'openai', group: 'Cloud', base: 'https://generativelanguage.googleapis.com/v1beta/openai', keyUrl: 'https://aistudio.google.com/apikey', keyHint: 'AIza…' },
  { id: 'mistral', name: 'Mistral AI', kind: 'openai', group: 'Cloud', base: 'https://api.mistral.ai/v1', keyUrl: 'https://console.mistral.ai/api-keys', keyHint: '' },
  { id: 'groq', name: 'Groq', kind: 'openai', group: 'Cloud', base: 'https://api.groq.com/openai/v1', keyUrl: 'https://console.groq.com/keys', keyHint: 'gsk_…' },
  { id: 'deepseek', name: 'DeepSeek', kind: 'openai', group: 'Cloud', base: 'https://api.deepseek.com/v1', keyUrl: 'https://platform.deepseek.com/api_keys', keyHint: 'sk-…' },
  { id: 'xai', name: 'xAI (Grok)', kind: 'openai', group: 'Cloud', base: 'https://api.x.ai/v1', keyUrl: 'https://console.x.ai/', keyHint: 'xai-…' },
  { id: 'openrouter', name: 'OpenRouter (many models)', kind: 'openai', group: 'Cloud', base: 'https://openrouter.ai/api/v1', keyUrl: 'https://openrouter.ai/keys', keyHint: 'sk-or-…' },
  { id: 'together', name: 'Together AI', kind: 'openai', group: 'Cloud', base: 'https://api.together.xyz/v1', keyUrl: 'https://api.together.ai/settings/api-keys', keyHint: '' },
  { id: 'ollama', name: 'Ollama (runs on your computer)', kind: 'openai', group: 'Local', base: 'http://localhost:11434/v1', noKey: true, editableBase: true, keyUrl: 'https://ollama.com/download' },
  { id: 'lmstudio', name: 'LM Studio (runs on your computer)', kind: 'openai', group: 'Local', base: 'http://localhost:1234/v1', noKey: true, editableBase: true, keyUrl: 'https://lmstudio.ai/' },
  { id: 'custom', name: 'Custom OpenAI-compatible API', kind: 'openai', group: 'Custom', base: '', keyOptional: true, editableBase: true, keyHint: 'optional' },
];
const byId = (id) => PROVIDERS.find((p) => p.id === id) || null;

// Anthropic models that accept the request options below (effort, server-side refusal fallback)
const ANTHROPIC_EFFORT = new Set(['claude-fable-5-1', 'claude-opus-5-5', 'claude-opus-5', 'claude-sonnet-5-5']);

// ---------------- pure helpers (unit-tested) ----------------

function isPrivateHost(host) {
  const h = host.replace(/^\[|\]$/g, '').toLowerCase();
  if (h === 'localhost' || h === '::1' || h.endsWith('.localhost') || h.endsWith('.local')) return true;
  const m = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(h);
  if (!m) return false;
  const [a, b] = [+m[1], +m[2]];
  return a === 127 || a === 10 || (a === 192 && b === 168) || (a === 172 && b >= 16 && b <= 31);
}

/** Accept https anywhere, http only on this computer or the local network. Returns a clean base URL. */
function validateBaseUrl(raw) {
  let u;
  try { u = new URL(String(raw || '').trim()); } catch (e) { throw new Error('Enter a full address, for example https://api.example.com/v1'); }
  if (u.username || u.password) throw new Error('Put the API key in the key field, not in the address.');
  if (u.protocol !== 'https:' && u.protocol !== 'http:') throw new Error('The address must start with https://');
  if (u.protocol === 'http:' && !isPrivateHost(u.hostname)) throw new Error('Use https:// for internet addresses (http:// is only allowed for this computer or your local network).');
  u.hash = ''; u.search = '';
  return u.toString().replace(/\/+$/, '');
}

const NON_CHAT = /embed|whisper|tts|speech|dall-?e|image|imagen|audio|realtime|moderation|transcri|rerank|guard|search-preview|computer-use|veo|lyria/i;

/** Turn a provider's model list into chat models, newest first. */
function chatModels(list) {
  const seen = new Set();
  return (list || [])
    .map((m) => ({ id: String(m.id || m.name || '').replace(/^models\//, ''), name: m.display_name || m.name || m.id, created: +new Date(m.created_at || 0) || (m.created ? m.created * 1000 : 0) }))
    .filter((m) => m.id && !NON_CHAT.test(m.id) && !seen.has(m.id) && seen.add(m.id))
    .sort((a, b) => b.created - a.created || a.id.localeCompare(b.id));
}

/** Text from an OpenAI-style chat response (string or content-part array). */
function chatText(json) {
  const msg = json && json.choices && json.choices[0] && json.choices[0].message;
  if (!msg) return '';
  if (typeof msg.content === 'string') return msg.content;
  if (Array.isArray(msg.content)) return msg.content.map((p) => (typeof p === 'string' ? p : p.text || '')).join('');
  return '';
}

const PREAMBLE = /^(sure|certainly|of course|okay|ok|absolutely|great|here is|here's|below is|this is)\b.*[:!.]$/i;

/** Keep only the outline: drop reasoning blocks, chatty lead-in lines, code fences, bullets, numbering and blank lines. */
function cleanOutline(text) {
  const lines = String(text || '')
    .replace(/<think>[\s\S]*?<\/think>/gi, '')
    .replace(/<\/?(outline|map)>/gi, '')
    .replace(/```[a-z]*\n?|```/gi, '')
    .split('\n')
    .map((l) => l.replace(/\t/g, '  ').replace(/\s+$/, ''))
    .filter((l) => l.trim());
  while (lines.length > 1 && PREAMBLE.test(lines[0].trim())) lines.shift();
  return lines.map((l) => l.replace(/^(\s*)(?:[-*+•]|\d+[.)]|#{1,6})\s+/, '$1').replace(/\*\*(.+?)\*\*/g, '$1')).join('\n');
}

const SYSTEM = 'You are the brainstorming engine of a mind-mapping app. Reply with ONLY an indented outline: one topic per line, indented with two spaces per level. No bullets, numbering, markdown, quotes or commentary. Keep every topic short (at most 7 words) and specific. Write in the same language as the user\'s request.';
const DEPTH = { quick: 'Give 4 to 6 main branches with 2 to 3 subtopics each.', standard: 'Give 5 to 7 main branches, each with 2 to 5 subtopics; go one level deeper where it adds real value.', deep: 'Give 6 to 8 main branches with 3 to 5 subtopics each, and a further level of detail under most subtopics.' };

function buildPrompt(req) {
  const clip = (s, n) => String(s || '').slice(0, n);
  const depth = DEPTH[req && req.depth] || DEPTH.standard;
  if (req && req.mode === 'expand') {
    return `Here is the current mind map as an indented outline:\n<map>\n${clip(req.outline, 6000)}\n</map>\n\nSuggest 4 to 7 NEW subtopics for the topic at this path: ${(req.path || []).map((x) => clip(x, 120)).join(' > ')}.\nDo not repeat existing subtopics: ${(req.existing || []).map((x) => clip(x, 80)).join('; ') || 'none'}.\nReply with the new subtopics only (no line for the parent). You may add up to 3 indented children under each.`;
  }
  if (req && req.mode === 'text') {
    return `Turn the following text into a mind map. The first line is a short central topic that names the subject. Organise the key ideas into branches and keep the facts from the text.\n${depth}\n<text>\n${clip(req.text, 30000)}\n</text>`;
  }
  return `Create a mind map about: ${clip(req && req.topic, 500)}\nThe first line is the central topic. ${depth}`;
}

// ---------------- storage ----------------

function store(app, safeStorage) {
  const dir = app.getPath('userData');
  const keysFile = path.join(dir, 'ai-keys.json');
  const cfgFile = path.join(dir, 'ai-config.json');
  const read = (f, d) => { try { return JSON.parse(fs.readFileSync(f, 'utf8')); } catch (e) { return d; } };
  const write = (f, v) => { const tmp = f + '.tmp'; fs.writeFileSync(tmp, JSON.stringify(v)); fs.renameSync(tmp, f); };
  // one-time migration from 1.0.0, which stored a single Anthropic key
  const legacy = path.join(dir, 'ai-key.bin');
  if (fs.existsSync(legacy)) {
    try { const k = read(keysFile, {}); if (!k.anthropic) { k.anthropic = fs.readFileSync(legacy).toString('base64'); write(keysFile, k); } fs.unlinkSync(legacy); } catch (e) { /* ignore */ }
  }
  return {
    config() { const c = read(cfgFile, {}); return { provider: byId(c.provider) ? c.provider : 'anthropic', models: c.models || {}, baseUrls: c.baseUrls || {} }; },
    setConfig(patch) {
      const c = this.config();
      const target = byId(patch.for || patch.provider || c.provider);
      if (patch.provider && byId(patch.provider)) c.provider = patch.provider;
      if (patch.model != null && target) c.models[target.id] = String(patch.model).trim().slice(0, 200);
      if (patch.baseUrl != null && target && target.editableBase) {
        if (String(patch.baseUrl).trim()) c.baseUrls[target.id] = validateBaseUrl(patch.baseUrl); else delete c.baseUrls[target.id];
      }
      write(cfgFile, c);
      return c;
    },
    hasKey(id) { return !!read(keysFile, {})[id]; },
    getKey(id) {
      const k = read(keysFile, {})[id];
      if (!k || !safeStorage.isEncryptionAvailable()) return null;
      try { return safeStorage.decryptString(Buffer.from(k, 'base64')); } catch (e) { return null; }
    },
    setKey(id, key) {
      if (!byId(id)) throw new Error('Unknown provider');
      if (!safeStorage.isEncryptionAvailable()) throw new Error('Secure key storage is not available on this computer.');
      const k = read(keysFile, {});
      k[id] = safeStorage.encryptString(String(key).trim()).toString('base64');
      write(keysFile, k);
    },
    clearKey(id) { const k = read(keysFile, {}); delete k[id]; write(keysFile, k); },
  };
}

// ---------------- provider calls ----------------

function friendly(status, detail, p, model) {
  const d = detail ? ` (${String(detail).slice(0, 200)})` : '';
  if (status === 401 || status === 403) return `${p.name} rejected the API key${d}. Check the key and try again.`;
  if (status === 404) return `${p.name} could not find the model “${model}”${d}. Pick another model from the list.`;
  if (status === 429) return `${p.name} says you have hit a rate limit or run out of credit${d}.`;
  if (status === 400) return `${p.name} could not process the request${d}.`;
  if (status >= 500) return `${p.name} is having problems right now${d}. Please try again shortly.`;
  return `${p.name} returned an error ${status}${d}.`;
}

async function openaiFetch(p, base, key, route, body, timeoutMs) {
  const headers = { 'Content-Type': 'application/json', Accept: 'application/json' };
  if (key) headers.Authorization = `Bearer ${key}`;
  if (p.id === 'openrouter') { headers['X-Title'] = 'Mindloom'; headers['HTTP-Referer'] = 'https://github.com/imranseoxpert/mindloom'; }
  let res;
  try {
    res = await fetch(`${base}${route}`, { method: body ? 'POST' : 'GET', headers, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(timeoutMs) });
  } catch (err) {
    if (err && err.name === 'TimeoutError') throw new Error(`${p.name} took too long to answer. Try a smaller or faster model.`);
    if (p.noKey) throw new Error(`Could not reach ${p.name} at ${base}. Make sure it is running and the address is right.`);
    throw new Error(`Could not reach ${p.name} (${base}). Check your internet connection and the address.`);
  }
  const text = await res.text();
  let json = null;
  try { json = JSON.parse(text); } catch (e) { /* not JSON */ }
  if (!res.ok) {
    const detail = (json && ((json.error && (json.error.message || json.error)) || json.message)) || text.slice(0, 200);
    throw new Error(friendly(res.status, typeof detail === 'string' ? detail : JSON.stringify(detail), p, body && body.model));
  }
  if (!json) throw new Error(`${p.name} sent an answer Mindloom could not read.`);
  return json;
}

function anthropicClient(key, cfg, smoke) {
  const mod = require('@anthropic-ai/sdk');
  const Anthropic = mod.default || mod;
  const opts = { apiKey: key, timeout: 180000, maxRetries: 2 };
  // a different base URL is only honoured by the automated self test (local mock server)
  if (smoke && cfg.baseUrls.anthropic) opts.baseURL = cfg.baseUrls.anthropic;
  return { client: new Anthropic(opts), Anthropic };
}

function anthropicError(err, Anthropic, p, model) {
  if (err instanceof Anthropic.AuthenticationError) return new Error(friendly(401, '', p, model));
  if (err instanceof Anthropic.PermissionDeniedError) return new Error(friendly(403, err.message, p, model));
  if (err instanceof Anthropic.NotFoundError) return new Error(friendly(404, '', p, model));
  if (err instanceof Anthropic.RateLimitError) return new Error(friendly(429, '', p, model));
  if (err instanceof Anthropic.APIConnectionError) return new Error(`Could not reach ${p.name}. Check your internet connection.`);
  if (err instanceof Anthropic.APIError) return new Error(friendly(err.status || 0, err.message, p, model));
  return err;
}

function register({ ipcMain, app, safeStorage, smoke }) {
  const S = store(app, safeStorage);
  const resolve = (id) => {
    const cfg = S.config();
    const p = byId(id || cfg.provider);
    if (!p) throw new Error('Choose an AI provider first.');
    const base = p.kind === 'openai' ? (cfg.baseUrls[p.id] || p.base) : null;
    if (p.kind === 'openai' && !base) throw new Error('Enter the address of your API (for example https://api.example.com/v1).');
    const key = S.getKey(p.id) || (p.id === 'anthropic' ? process.env.ANTHROPIC_API_KEY : null);
    if (!key && !p.noKey && !p.keyOptional) throw new Error(`Add your ${p.name} API key first.`);
    return { cfg, p, base: base ? validateBaseUrl(base) : null, key };
  };

  ipcMain.handle('ai-config', () => {
    const cfg = S.config();
    return {
      provider: cfg.provider, models: cfg.models, baseUrls: cfg.baseUrls,
      providers: PROVIDERS.map((p) => ({ id: p.id, name: p.name, group: p.group, keyUrl: p.keyUrl, keyHint: p.keyHint, noKey: !!p.noKey, keyOptional: !!p.keyOptional, editableBase: !!p.editableBase, base: p.base || '', defaultModel: p.defaultModel || '', hasKey: S.hasKey(p.id) || (p.id === 'anthropic' && !!process.env.ANTHROPIC_API_KEY) })),
      secure: safeStorage.isEncryptionAvailable(),
    };
  });
  ipcMain.handle('ai-config-set', (e, patch) => { S.setConfig(patch || {}); return true; });
  ipcMain.handle('ai-key-set', (e, id, key) => { if (!String(key || '').trim()) throw new Error('Paste a key first.'); S.setKey(id, key); return true; });
  ipcMain.handle('ai-key-clear', (e, id) => { S.clearKey(id); return true; });

  ipcMain.handle('ai-models', async (e, id) => {
    const { cfg, p, base, key } = resolve(id);
    if (p.kind === 'anthropic') {
      const { client, Anthropic } = anthropicClient(key, cfg, smoke);
      try {
        const out = [];
        for await (const m of client.models.list()) { out.push(m); if (out.length >= 200) break; }
        return chatModels(out);
      } catch (err) { throw anthropicError(err, Anthropic, p, ''); }
    }
    const json = await openaiFetch(p, base, key, '/models', null, 30000);
    return chatModels(json.data || json.models || []);
  });

  ipcMain.handle('ai', async (e, req) => {
    const { cfg, p, base, key } = resolve(req && req.provider);
    const model = (req && req.model) || cfg.models[p.id] || p.defaultModel;
    if (!model) throw new Error(`Choose a ${p.name} model first (press “Load models”).`);
    const prompt = buildPrompt(req || {});
    let text;
    if (p.kind === 'anthropic') {
      const { client, Anthropic } = anthropicClient(key, cfg, smoke);
      const params = { model, max_tokens: 16000, system: SYSTEM, messages: [{ role: 'user', content: prompt }] };
      const modern = ANTHROPIC_EFFORT.has(model);
      if (modern) Object.assign(params, { betas: ['server-side-fallback-2026-07-01'], fallbacks: 'default', output_config: { effort: 'medium' } });
      let response;
      try { response = modern ? await client.beta.messages.create(params) : await client.messages.create(params); } catch (err) { throw anthropicError(err, Anthropic, p, model); }
      if (response.stop_reason === 'refusal') throw new Error('The model declined this request. Try rephrasing it.');
      text = response.content.filter((b) => b.type === 'text').map((b) => b.text).join('\n');
    } else {
      const json = await openaiFetch(p, base, key, '/chat/completions', { model, stream: false, messages: [{ role: 'system', content: SYSTEM }, { role: 'user', content: prompt }] }, 240000);
      const choice = json.choices && json.choices[0];
      if (choice && choice.finish_reason === 'content_filter') throw new Error('The model declined this request. Try rephrasing it.');
      text = chatText(json);
    }
    const outline = cleanOutline(text);
    if (!outline.trim()) throw new Error('The model returned an empty answer. Try again or pick another model.');
    S.setConfig({ for: p.id, model });
    return { outline, provider: p.id, model };
  });
}

module.exports = { register, PROVIDERS, validateBaseUrl, isPrivateHost, chatModels, chatText, cleanOutline, buildPrompt };
