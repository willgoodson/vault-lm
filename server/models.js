// Model providers behind one interface: local Ollama, Anthropic (Claude), and any OpenAI-compatible
// server (OpenAI, LM Studio, OpenRouter, Groq, llama.cpp, vLLM…). Settings live in data/models.json
// (app-wide, not per vault). API keys never leave the server.
import fs from 'node:fs/promises';
import path from 'node:path';
import { Ollama } from 'ollama';
import Anthropic from '@anthropic-ai/sdk';
import { DATA } from './config.js';

const file = path.join(DATA, 'models.json');
const DEFAULTS = {
  chat: { provider: 'ollama', model: process.env.CHAT_MODEL || 'llama3.1:8b' },
  embed: { provider: 'ollama', model: process.env.EMBED_MODEL || 'nomic-embed-text' },
  ollama: { baseUrl: 'http://127.0.0.1:11434' },
  anthropic: { apiKey: '', effort: 'medium' },
  openai: { baseUrl: 'https://api.openai.com/v1', apiKey: '', label: 'OpenAI-compatible' },
};
let cfg = null;

export async function loadConfig() {
  if (cfg) return cfg;
  const saved = JSON.parse(await fs.readFile(file, 'utf8').catch(() => '{}'));
  cfg = Object.fromEntries(Object.entries(DEFAULTS).map(([k, v]) => [k, { ...v, ...(saved[k] || {}) }]));
  return cfg;
}
async function saveConfig() {
  await fs.mkdir(DATA, { recursive: true });
  await fs.writeFile(file, JSON.stringify(cfg, null, 2), { mode: 0o600 });
  await fs.chmod(file, 0o600).catch(() => {});
}

const anthropicKey = () => cfg.anthropic.apiKey || process.env.ANTHROPIC_API_KEY || '';
const openaiKey = () => cfg.openai.apiKey || process.env.OPENAI_API_KEY || '';
const mask = (k) => (k ? `${k.slice(0, 7)}…${k.slice(-4)}` : '');

// What the UI sees: everything except raw keys.
export async function publicConfig() {
  await loadConfig();
  return {
    chat: cfg.chat, embed: cfg.embed,
    ollama: cfg.ollama,
    anthropic: { effort: cfg.anthropic.effort, hasKey: !!anthropicKey(), keyHint: mask(anthropicKey()), fromEnv: !cfg.anthropic.apiKey && !!process.env.ANTHROPIC_API_KEY },
    openai: { baseUrl: cfg.openai.baseUrl, label: cfg.openai.label, hasKey: !!openaiKey(), keyHint: mask(openaiKey()), fromEnv: !cfg.openai.apiKey && !!process.env.OPENAI_API_KEY },
  };
}

export async function updateConfig(patch) {
  await loadConfig();
  for (const k of ['chat', 'embed', 'ollama', 'anthropic', 'openai']) {
    if (!patch[k]) continue;
    const next = { ...patch[k] };
    if ('apiKey' in next && next.apiKey == null) delete next.apiKey; // null = keep the current key
    cfg[k] = { ...cfg[k], ...next };
  }
  clients = {};
  await saveConfig();
  return publicConfig();
}

export const embedId = () => `${cfg.embed.provider}:${cfg.embed.model.replace(/:latest$/, "")}`;
export const chatLabel = () => (cfg.chat.provider === 'ollama' ? cfg.chat.model : `${cfg.chat.model} (${cfg.chat.provider === 'anthropic' ? 'Anthropic' : cfg.openai.label})`);
export const isCloud = (provider = cfg.chat.provider) => provider === 'anthropic' || (provider === 'openai' && !/localhost|127\.0\.0\.1|0\.0\.0\.0/.test(cfg.openai.baseUrl));

let clients = {};
const ollama = () => (clients.ollama ??= new Ollama({ host: cfg.ollama.baseUrl }));
const anthropic = () => {
  if (!anthropicKey()) throw Object.assign(new Error('No Anthropic API key yet. Add one in Models (or set ANTHROPIC_API_KEY)'), { statusCode: 400 });
  return (clients.anthropic ??= new Anthropic({ apiKey: anthropicKey() }));
};

// ---------- chat
// Models that take the server-side refusal fallback (`fallbacks: "default"`) on the Claude API.
const FALLBACK_MODELS = /^claude-(fable-5-1|opus-5-5|opus-5|sonnet-5-5)$/;

/**
 * messages: [{role: 'system'|'user'|'assistant', content}]
 * opts: { json?: bool, onToken?: fn, temperature?, numCtx? }  → full text
 */
export async function chat(messages, opts = {}) {
  await loadConfig();
  const { provider, model } = cfg.chat;
  if (provider === 'anthropic') return anthropicChat(model, messages, opts);
  if (provider === 'openai') return openaiChat(model, messages, opts);
  return ollamaChat(model, messages, opts);
}

async function ollamaChat(model, messages, { json, onToken, temperature, numCtx = 16384 }) {
  const options = { num_ctx: numCtx, ...(temperature != null && { temperature }) };
  if (!onToken) {
    const r = await ollama().chat({ model, messages, options, ...(json && { format: 'json' }) });
    return r.message.content;
  }
  const stream = await ollama().chat({ model, messages, options, stream: true, ...(json && { format: 'json' }) });
  let text = '';
  for await (const part of stream) { text += part.message.content; onToken(part.message.content); }
  return text;
}

async function anthropicChat(model, messages, { json, onToken }) {
  const system = messages.filter((m) => m.role === 'system').map((m) => m.content).join('\n\n');
  const turns = messages.filter((m) => m.role !== 'system').map((m) => ({ role: m.role, content: m.content }));
  const params = {
    model,
    max_tokens: 64000,
    system: json ? `${system}\n\nRespond with a single valid JSON object and nothing else.` : system,
    messages: turns,
    output_config: { effort: cfg.anthropic.effort || 'medium' },
  };
  // Streaming avoids HTTP timeouts on long outputs. Newer models reject `temperature`, so it's never sent.
  const stream = FALLBACK_MODELS.test(model)
    ? anthropic().beta.messages.stream({ ...params, betas: ['server-side-fallback-2026-07-01'], fallbacks: 'default' })
    : anthropic().messages.stream(params);
  let text = '';
  try {
    for await (const event of stream) {
      if (event.type === 'content_block_delta' && event.delta.type === 'text_delta') {
        text += event.delta.text;
        onToken?.(event.delta.text);
      }
    }
    const final = await stream.finalMessage();
    if (final.stop_reason === 'refusal') throw new Error('The model declined to answer this request.');
  } catch (e) {
    throw new Error(anthropicError(e));
  }
  return json ? extractJSON(text) : text;
}

function anthropicError(e) {
  if (e instanceof Anthropic.AuthenticationError) return 'Anthropic rejected the API key';
  if (e instanceof Anthropic.PermissionDeniedError) return 'This API key can’t use that model';
  if (e instanceof Anthropic.NotFoundError) return 'Model not found on Anthropic';
  if (e instanceof Anthropic.RateLimitError) return 'Anthropic rate limit reached. Try again shortly';
  if (e instanceof Anthropic.APIConnectionError) return 'Couldn’t reach Anthropic (offline?)';
  if (e instanceof Anthropic.APIError) return `Anthropic error ${e.status}: ${e.message}`;
  return e.message;
}

// Some providers wrap JSON in prose or fences; pull out the first complete object.
function extractJSON(text) {
  const t = text.replace(/^```(?:json)?\s*|```\s*$/g, '').trim();
  const start = t.indexOf('{');
  if (start < 0) return t;
  let depth = 0, inStr = false, esc = false;
  for (let i = start; i < t.length; i++) {
    const c = t[i];
    if (inStr) { if (esc) esc = false; else if (c === '\\') esc = true; else if (c === '"') inStr = false; continue; }
    if (c === '"') inStr = true; else if (c === '{') depth++; else if (c === '}' && --depth === 0) return t.slice(start, i + 1);
  }
  return t.slice(start);
}

async function openaiFetch(p, body, method = 'POST') {
  const res = await fetch(cfg.openai.baseUrl.replace(/\/+$/, '') + p, {
    method,
    headers: { 'content-type': 'application/json', ...(openaiKey() && { authorization: `Bearer ${openaiKey()}` }) },
    ...(body && { body: JSON.stringify(body) }),
  }).catch((e) => { throw new Error(`Couldn’t reach ${cfg.openai.baseUrl} (${e.cause?.code || e.message})`); });
  if (!res.ok) {
    const msg = await res.text().catch(() => '');
    throw new Error(`${cfg.openai.label} error ${res.status}: ${(() => { try { return JSON.parse(msg).error?.message || msg; } catch { return msg; } })().slice(0, 300)}`);
  }
  return res;
}

async function openaiChat(model, messages, { json, onToken, temperature }) {
  const body = { model, messages, stream: true, ...(temperature != null && { temperature }), ...(json && { response_format: { type: 'json_object' } }) };
  let res;
  try { res = await openaiFetch('/chat/completions', body); }
  catch (e) {
    if (json && /response_format/i.test(e.message)) { delete body.response_format; res = await openaiFetch('/chat/completions', body); }
    else throw e;
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = '', text = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += value;
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const l of lines) {
      const d = l.replace(/^data:\s*/, '').trim();
      if (!d || d === '[DONE]' || !l.startsWith('data:')) continue;
      try {
        const delta = JSON.parse(d).choices?.[0]?.delta?.content;
        if (delta) { text += delta; onToken?.(delta); }
      } catch {}
    }
  }
  return json ? extractJSON(text) : text;
}

// ---------- embeddings (Anthropic has no embeddings API, so: Ollama or OpenAI-compatible)
export async function embed(texts, kind = 'search_document') {
  await loadConfig();
  const { provider, model } = cfg.embed;
  // nomic-embed models expect task prefixes; other models don't.
  const input = /nomic/i.test(model) ? texts.map((t) => `${kind}: ${t}`) : texts;
  if (provider === 'openai') {
    const r = await (await openaiFetch('/embeddings', { model, input })).json();
    return r.data.sort((a, b) => a.index - b.index).map((d) => d.embedding);
  }
  return (await ollama().embed({ model, input })).embeddings;
}

// ---------- discovery
const EMBED_HINT = /embed|bge|e5-|gte|minilm|mxbai|nomic-bert|arctic-embed/i;

export async function listModels(provider) {
  await loadConfig();
  if (provider === 'ollama') {
    const r = await ollama().list();
    return r.models.map((m) => ({
      id: m.name, name: m.name, size: m.size, params: m.details?.parameter_size, quant: m.details?.quantization_level,
      kind: EMBED_HINT.test(m.name) || /bert/i.test(m.details?.family || '') ? 'embed' : 'chat',
    })).sort((a, b) => a.name.localeCompare(b.name));
  }
  if (provider === 'anthropic') {
    const out = [];
    try { for await (const m of anthropic().models.list()) out.push({ id: m.id, name: m.display_name || m.id, kind: 'chat', context: m.max_input_tokens, created: m.created_at }); }
    catch (e) { throw new Error(anthropicError(e)); }
    return out;
  }
  const r = await (await openaiFetch('/models', null, 'GET')).json();
  return (r.data || []).map((m) => ({ id: m.id, name: m.id, kind: EMBED_HINT.test(m.id) ? 'embed' : 'chat' })).sort((a, b) => a.id.localeCompare(b.id));
}

export async function testProvider(provider) {
  const t = Date.now();
  const models = await listModels(provider);
  return { ok: true, models: models.length, ms: Date.now() - t };
}

// Download a local model, reporting progress.
export async function pullModel(name, onProgress) {
  await loadConfig();
  const stream = await ollama().pull({ model: name, stream: true });
  for await (const p of stream) onProgress?.(p);
}
