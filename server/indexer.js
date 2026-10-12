import fs from 'node:fs/promises';
import path from 'node:path';
import * as lancedb from '@lancedb/lancedb';
import { SKIP_DIRS } from './config.js';
import * as models from './models.js';
import { vault } from './vaults.js';

const MAX_CHARS = 3200; // ~800 tokens
const cache = { key: '', db: null, table: null };
export const status = { running: false, done: 0, total: 0, chunks: 0 };

// Each embedding model gets its own index (vectors from different models aren't comparable).
export async function indexDir() {
  const cfg = await models.loadConfig();
  const id = models.embedId();
  return id === 'ollama:nomic-embed-text' ? path.join(vault.data, 'lancedb') : path.join(vault.data, 'lancedb-' + id.replace(/[^\w.-]+/g, '_'));
}

async function open() {
  const dir = await indexDir();
  if (cache.key !== dir) {
    cache.db = await lancedb.connect(dir);
    cache.table = (await cache.db.tableNames()).includes('chunks') ? await cache.db.openTable('chunks') : null;
    cache.key = dir;
  }
  return cache;
}

export async function getTable() { return (await open()).table; }

export const embed = (texts, kind) => models.embed(texts, kind);

export async function listNotes(dir = vault.path) {
  const out = [];
  for (const ent of await fs.readdir(dir, { withFileTypes: true }).catch(() => [])) {
    if (ent.name.startsWith('.') || SKIP_DIRS.has(ent.name)) continue;
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) out.push(...(await listNotes(full)));
    else if (ent.name.endsWith('.md')) out.push(full);
  }
  return out;
}

export function notebookOf(rel) {
  const parts = rel.split(path.sep);
  return parts.length > 1 ? parts[0] : '(root)';
}

export function chunkNote(text) {
  text = text.replace(/^---\n[\s\S]*?\n---\n/, '');
  const chunks = [];
  let heading = '';
  let buf = [];
  const flush = () => {
    const body = buf.join('\n').trim();
    buf = [];
    if (body.split('\n').every((l) => !l.trim() || /^#{1,6}\s/.test(l))) return;
    for (let i = 0; i < body.length; i += MAX_CHARS) {
      const piece = body.slice(i, i + MAX_CHARS).trim();
      if (piece) chunks.push({ heading, text: piece });
    }
  };
  for (const line of text.split('\n')) {
    const m = line.match(/^#{1,6}\s+(.*)/);
    if (m) { flush(); heading = m[1].trim(); }
    buf.push(line);
  }
  flush();
  return chunks;
}

async function rowsForFile(full, mtime) {
  const rel = path.relative(vault.path, full);
  const note = path.basename(rel, '.md');
  const chunks = chunkNote(await fs.readFile(full, 'utf8'));
  if (!chunks.length) return [];
  const vecs = await embed(chunks.map((c) => `${note} ${c.heading}\n${c.text}`));
  return chunks.map((c, i) => ({
    vector: vecs[i], text: c.text, heading: c.heading, note, path: rel,
    notebook: notebookOf(rel), mtime,
  }));
}

export const esc = (s) => s.replace(/'/g, "''");

// Full-text (BM25) index over chunk text, for hybrid keyword + vector search.
async function ensureFts(table, changed) {
  try {
    const has = (await table.listIndices()).some((i) => i.columns?.includes('text') && /fts|inverted/i.test(i.indexType || ''));
    if (has && !changed) return;
    await table.createIndex('text', { config: lancedb.Index.fts({ withPosition: false }), replace: true });
  } catch (e) { console.error('FTS index failed:', e.message); }
}

// Turn free text into a safe keyword query (FTS syntax chars stripped).
export const ftsQuery = (q) => String(q).toLowerCase().replace(/[^\p{L}\p{N}\s'-]/gu, ' ').split(/\s+/).filter((w) => w.length > 2).slice(0, 24).join(' ');

// Incremental: only re-embeds files whose mtime changed; drops deleted files.
export async function indexVault({ log = console.log, onProgress = () => {} } = {}) {
  const target = vault.data;
  const c = await open();
  const dirAtStart = cache.key;
  const known = new Map();
  if (c.table) for (const r of await c.table.query().select(['path', 'mtime']).toArray()) known.set(r.path, r.mtime);
  const files = await listNotes();
  Object.assign(status, { running: true, done: 0, total: files.length });
  const seen = new Set();
  let changed = 0;
  try {
    for (const full of files) {
      if (vault.data !== target || cache.key !== dirAtStart) return { aborted: true }; // vault or embedding model switched mid-run
      const rel = path.relative(vault.path, full);
      seen.add(rel);
      status.done++;
      onProgress(status);
      const mtime = (await fs.stat(full)).mtimeMs;
      if (known.get(rel) === mtime) continue;
      const rows = await rowsForFile(full, mtime).catch((e) => (log(`skip ${rel}: ${e.message}`), []));
      if (c.table) await c.table.delete(`path = '${esc(rel)}'`);
      if (rows.length) {
        if (!c.table) c.table = await c.db.createTable('chunks', rows);
        else await c.table.add(rows);
      }
      changed++;
      log(`indexed ${rel} (${rows.length} chunks)`);
    }
    if (c.table) for (const rel of known.keys()) {
      if (!seen.has(rel)) { await c.table.delete(`path = '${esc(rel)}'`); log(`removed ${rel}`); }
    }
    if (c.table) await ensureFts(c.table, changed > 0);
    status.chunks = c.table ? await c.table.countRows() : 0;
    return { files: files.length, changed, chunks: status.chunks };
  } finally {
    status.running = false;
  }
}

// Folder tree of the vault (all files, for the browser).
export async function tree(dir = vault.path) {
  const out = [];
  const ents = (await fs.readdir(dir, { withFileTypes: true }).catch(() => []))
    .filter((e) => !e.name.startsWith('.'))
    .sort((a, b) => (b.isDirectory() - a.isDirectory()) || a.name.localeCompare(b.name));
  for (const e of ents) {
    const full = path.join(dir, e.name);
    const rel = path.relative(vault.path, full);
    if (e.isDirectory()) out.push({ type: 'dir', name: e.name, path: rel, children: await tree(full) });
    else out.push({ type: 'file', name: e.name, path: rel });
  }
  return out;
}
