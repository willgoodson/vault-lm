import { getTable, embed, esc, ftsQuery } from './indexer.js';
import { getOverview } from './overview.js';
import { listNotebooks, targetWhere, notebookWhere, originOf, topicSourcesDir } from './layout.js';
import { K_PRIMARY } from './config.js';

const strip = ({ vector, ...r }) => r;
const isNotebookScope = (s) => !s.section && !s.topic;

/**
 * Always rooted in the notebook. scope = { notebook, section?, topic?, mode }
 *  reference (default) – the selection first, then the most relevant material from the rest of the notebook
 *  focused             – only the selection, plus the notebook overview as background context
 */
export async function retrieve(question, scope = {}) {
  const table = await getTable();
  if (!table || !scope.notebook) return [];
  const mode = scope.mode === 'focused' ? 'focused' : 'reference';
  const [qv] = await embed([question], 'search_query');
  const kw = ftsQuery(question);
  const search = (where, k) => hybrid(table, qv, kw, where, k);
  const nbTree = (await listNotebooks()).find((n) => n.path === scope.notebook);
  const tag = (rows, kind) => rows.map((r) => ({ ...strip(r), kind, reference: kind !== 'primary', origin: originOf(nbTree, r.path) }));
  // Keep the top hits, anything semantically close, and any exact keyword match.
  const relevant = (rows) => rows.filter((r, i) => i < 2 || r._distance <= 0.6 || (r._ftsRank != null && r._ftsRank < 3));

  if (isNotebookScope(scope)) return tag(relevant(await search(notebookWhere(scope), K_PRIMARY + 2)), 'primary');

  const primary = tag(relevant(await search(targetWhere(scope), K_PRIMARY)), 'primary');
  if (mode === 'focused') {
    const ov = await getOverview(scope.notebook);
    return ov ? [...primary, { note: ov.note, heading: '', path: ov.path, text: ov.text, kind: 'overview', reference: true, origin: 'overview' }] : primary;
  }
  const rest = await search(`${notebookWhere(scope)} AND NOT ${targetWhere(scope)}`, 8);
  // Rest of the notebook: only clearly related chunks (close in meaning, or an exact keyword hit).
  const close = rest.filter((r) => r._distance <= 0.55 || (r._ftsRank != null && r._ftsRank < 3));
  return [...primary, ...tag(close.slice(0, 6), 'notebook')];
}

/**
 * Hybrid search: cosine vector search + BM25 full-text, fused with reciprocal rank fusion (k=60).
 * Rows keep `_distance` (vector, when present) and `_ftsRank` (keyword rank, when present).
 */
export async function hybrid(table, qv, kw, where, k) {
  const vq = table.vectorSearch(qv).distanceType('cosine').limit(k * 2);
  const vec = await (where ? vq.where(where) : vq).toArray();
  let fts = [];
  if (kw) {
    try {
      const fq = table.query().fullTextSearch(kw).limit(k * 2);
      fts = await (where ? fq.where(where) : fq).toArray();
    } catch { /* no FTS index yet — vector only */ }
  }
  const key = (r) => `${r.path}\u0000${r.heading}\u0000${r.text.slice(0, 80)}`;
  const fused = new Map();
  vec.forEach((r, i) => fused.set(key(r), { ...r, _rrf: 1 / (60 + i) }));
  fts.forEach((r, i) => {
    const kk = key(r);
    const cur = fused.get(kk);
    if (cur) { cur._rrf += 1 / (60 + i); cur._ftsRank = i; }
    else fused.set(kk, { ...r, _distance: r._distance ?? 1, _rrf: 1 / (60 + i), _ftsRank: i });
  });
  return [...fused.values()].sort((a, b) => b._rrf - a._rrf).slice(0, k).map(({ _score, ...r }) => r);
}

// All chunks of a scope in path order, for study tools.
export async function scopeText(scope, maxChars = 24000) {
  const table = await getTable();
  if (!table) return '';
  const where = scope.section || scope.topic ? targetWhere(scope) : notebookWhere(scope);
  const rows = await table.query().where(where).select(['note', 'heading', 'text', 'path']).toArray();
  rows.sort((a, b) => a.path.localeCompare(b.path));
  let out = '';
  for (const r of rows) {
    const block = `\n### [[${r.note}${r.heading ? '#' + r.heading : ''}]]\n${r.text}\n`;
    if (out.length + block.length > maxChars) break;
    out += block;
  }
  return out;
}

export async function pathForNote(name) {
  const table = await getTable();
  if (!table) return null;
  const [r] = await table.query().where(`note = '${esc(name)}'`).select(['path']).limit(1).toArray();
  return r?.path || null;
}

// Average chunk embedding per note → nearest notes (for "Related topics").
export async function relatedNotes(notePath, { prefix, limit = 8 } = {}) {
  const table = await getTable();
  if (!table) return [];
  const own = await table.query().where(`path = '${esc(notePath)}'`).select(['vector']).toArray();
  if (!own.length) return [];
  // LanceDB returns Arrow vectors; convert before doing math on them.
  const vecs = own.map((r) => Array.from(r.vector));
  const avg = new Array(vecs[0].length).fill(0);
  for (const v of vecs) for (let i = 0; i < v.length; i++) avg[i] += v[i] / vecs.length;
  let where = `path != '${esc(notePath)}' AND NOT starts_with(path, '${esc(topicSourcesDir(notePath))}/')`;
  if (prefix) where += ` AND starts_with(path, '${esc(prefix)}/')`;
  const q = table.vectorSearch(avg).distanceType('cosine').where(where).limit(limit * 6);
  const best = new Map();
  for (const r of await q.toArray()) {
    if (r.path.includes('/_studio/')) continue;
    if (!best.has(r.path) || best.get(r.path)._distance > r._distance) best.set(r.path, r);
  }
  return [...best.values()].sort((a, b) => a._distance - b._distance).slice(0, limit)
    .map((r) => ({ path: r.path, note: r.note, heading: r.heading, score: +(1 - r._distance).toFixed(3) }));
}
