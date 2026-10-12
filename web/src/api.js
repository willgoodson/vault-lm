// One API surface for the UI. In the desktop app (Tauri) calls go over IPC to the Rust core;
// in a plain browser (dev/testing) they go over HTTP. Route paths and JSON shapes are identical.
import { invoke, Channel } from '@tauri-apps/api/core';

export const isDesktop = typeof window !== 'undefined' && !!window.__TAURI_INTERNALS__;

const toB64 = (buf) => {
  const bytes = new Uint8Array(buf);
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};

// FormData → { files: [{ name, data(base64) }], ...fields } for IPC.
async function formToJson(fd) {
  const out = { files: [] };
  for (const [k, v] of fd.entries()) {
    if (v instanceof Blob) out.files.push({ name: v.name || 'file', type: v.type, data: toB64(await v.arrayBuffer()) });
    else out[k] = v;
  }
  return out;
}

function apiError(e) {
  const err = new Error(e?.message || String(e));
  if (e && typeof e === 'object') Object.assign(err, e);
  return err;
}

export async function api(url, body, method) {
  method = method || (body ? 'POST' : 'GET');
  if (isDesktop) {
    const u = new URL(url, 'http://app');
    const query = Object.fromEntries(u.searchParams.entries());
    const payload = body instanceof FormData ? await formToJson(body) : body ?? null;
    try { return await invoke('api', { method, path: u.pathname, query, body: payload }); }
    catch (e) { throw apiError(e); }
  }
  const opts = { method };
  if (body instanceof FormData) opts.body = body;
  else if (body) { opts.body = JSON.stringify(body); opts.headers = { 'content-type': 'application/json' }; }
  const res = await fetch(url, opts);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw apiError({ status: res.status, ...data, message: data.message || res.statusText });
  return data;
}

// Streaming responses (chat). Desktop: a Tauri Channel delivering the same event objects.
export async function stream(url, body, onEvent) {
  if (isDesktop) {
    const ch = new Channel();
    ch.onmessage = onEvent;
    try { await invoke('stream', { path: new URL(url, 'http://app').pathname, body, onEvent: ch }); }
    catch (e) { throw apiError(e); }
    return;
  }
  const res = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += value;
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const l of lines) if (l.trim()) onEvent(JSON.parse(l));
  }
}
