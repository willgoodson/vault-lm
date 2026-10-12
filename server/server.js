import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import Fastify from 'fastify';
import multipart from '@fastify/multipart';
import fstatic from '@fastify/static';
import chokidar from 'chokidar';
import { SKIP_DIRS } from './config.js';
import * as models from './models.js';
import { vault, loadVault, saveVault, setVault, knownVaults, isIndexed } from './vaults.js';
import { indexVault, tree, getTable, status as indexStatus } from './indexer.js';
import { listNotebooks, getSettings, saveSettings, topFolders, baseOf, sourcesDirFor } from './layout.js';
import { refreshOverviews, overviewStatus } from './overview.js';
import { chat, brief, followUps, BRIEFS } from './llm.js';
import { addUrl, addFile, reprocess } from './sources.js';
import { audioOverview } from './audio.js';
import * as study from './study.js';
import * as files from './files.js';
import * as links from './links.js';
import { subscribe, emit } from './events.js';
import { loadUiState, saveUiState } from './uistate.js';
import { pathForNote, relatedNotes } from './retrieve.js';
import { loadMeta, setMeta, rekeyMeta, dropMeta } from './meta.js';
import { rekeyCards, setNotebookResolver } from './srs.js';
import * as progress from './progress.js';
import { loadTemplates, saveTemplates, resetTemplates, fill } from './templates.js';

await loadVault();
setNotebookResolver(async (p) => (await listNotebooks()).find((n) => p === n.path || p.startsWith(n.path + '/'))?.path);
const app = Fastify({ logger: false, bodyLimit: 20 * 1024 * 1024 });
await app.register(multipart, { limits: { fileSize: 100 * 1024 * 1024 } });
const dist = path.resolve(import.meta.dirname, '../web/dist');
if (await fs.stat(dist).catch(() => null)) await app.register(fstatic, { root: dist });

app.setErrorHandler((err, _req, reply) =>
  reply.code(err.statusCode || 500).send({ message: err.message, ...(err.current != null && { current: err.current, mtime: err.mtime }) }));

const home = (p) => path.resolve(String(p).replace(/^~(?=\/|$)/, os.homedir()));
const scopeOf = (b) => ({ notebook: b.notebook, section: b.section || undefined, topic: b.topic || undefined, mode: b.mode });

// ---------- indexing, link index, watcher (re-pointed on vault switch)
let indexing = Promise.resolve();
const log = (m) => console.log(`[${vault.name}] ${m}`);
let lastEmit = 0;
const indexProgress = (s) => { if (Date.now() - lastEmit > 300 || s.done === s.total) { lastEmit = Date.now(); emit('index', s); } };
function reindex() {
  indexing = indexing
    .then(() => links.build())
    .then(() => indexVault({ log, onProgress: indexProgress }))
    .then(() => emit('index', { ...indexStatus, running: false }))
    .catch((e) => console.error(e));
  indexing.then(async () => {
    emit('tree');
    const poll = setInterval(() => emit('overview', { running: overviewStatus.running }), 2000);
    await refreshOverviews(log);
    clearInterval(poll);
    emit('overview', { running: null });
  });
  return indexing;
}

let watcher, timer;
const pending = new Set();
async function watch() {
  await watcher?.close();
  watcher = chokidar.watch(vault.path, {
    ignored: (p) => path.relative(vault.path, p).split(path.sep).some((s) => (s.startsWith('.') && s.length > 1)),
    ignoreInitial: true,
    awaitWriteFinish: { stabilityThreshold: 200, pollInterval: 50 },
  }).on('all', async (event, full) => {
    const rel = path.relative(vault.path, full);
    const mtime = (await fs.stat(full).catch(() => null))?.mtimeMs ?? null;
    emit('file', { event, path: rel, mtime });
    pending.add(rel);
    clearTimeout(timer);
    timer = setTimeout(() => {
      pending.clear();
      emit('tree');
      reindex();
    }, 1200);
  });
}

// ---------- models (app-wide)
app.get('/api/models', async () => models.publicConfig());
app.put('/api/models', async (req) => {
  const before = models.embedId();
  const cfg = await models.updateConfig(req.body || {});
  const reindexed = models.embedId() !== before;
  if (reindexed) reindex(); // new embedding model → build (or reuse) its own index
  emit('models', { chat: models.chatLabel(), cloud: models.isCloud() });
  return { ...cfg, reindexed };
});
app.get('/api/models/list', async (req) => {
  try { return await models.listModels(String(req.query.provider || 'ollama')); }
  catch (e) { throw files.httpError(400, e.message); }
});
app.post('/api/models/test', async (req) => {
  // Test with the submitted settings without saving them unless they work.
  const before = JSON.parse(JSON.stringify(await models.loadConfig()));
  const patch = req.body.settings ? { [req.body.provider]: req.body.settings } : {};
  try {
    if (Object.keys(patch).length) await models.updateConfig(patch);
    return await models.testProvider(req.body.provider);
  } catch (e) {
    if (Object.keys(patch).length) await models.updateConfig({ [req.body.provider]: { ...before[req.body.provider] } });
    throw files.httpError(400, e.message);
  }
});
app.post('/api/models/pull', async (req) => {
  const name = String(req.body.name || '').trim();
  if (!/^[\w.\-/:]+$/.test(name)) throw files.httpError(400, 'Invalid model name');
  const id = `pull-${Date.now()}`;
  models.pullModel(name, (p) => emit('job', { id, label: `Downloading ${name}`, done: p.completed || 0, total: p.total || 1, note: p.status }))
    .then(() => emit('job', { id, label: name, finished: true }), (e) => emit('job', { id, label: name, finished: true, error: e.message }));
  return { started: true };
});

// ---------- events + ui state
app.get('/api/events', (req, reply) => { subscribe(reply); emit('index', indexStatus); });
app.get('/api/ui-state', async () => loadUiState());
app.put('/api/ui-state', async (req) => { await saveUiState(req.body || {}); return { ok: true }; });

// ---------- vaults
app.get('/api/info', async () => ({
  vault: vault.path, vaultName: vault.name, model: models.chatLabel(), provider: (await models.loadConfig()).chat.provider, cloud: models.isCloud(), embedModel: models.embedId(), briefs: Object.keys(BRIEFS),
}));

app.get('/api/vaults', async () => {
  const list = await knownVaults();
  if (!list.some((v) => v.path === vault.path)) list.unshift({ name: vault.name, path: vault.path });
  return Promise.all(list.map(async (v) => ({ ...v, current: v.path === vault.path, indexed: await isIndexed(v.path) })));
});

async function switchVault(p) {
  const stat = await fs.stat(p).catch(() => null);
  if (!stat?.isDirectory()) throw files.httpError(400, `Not a folder: ${p}`);
  setVault(p);
  await fs.mkdir(vault.data, { recursive: true });
  await saveVault();
  await watch();
  reindex();
  return { name: vault.name, path: vault.path };
}
app.post('/api/vaults/select', async (req) => switchVault(home(req.body.path)));
app.post('/api/vaults/create', async (req) => {
  const p = home(req.body.path);
  if (await fs.stat(p).catch(() => null)) throw files.httpError(409, `${p} already exists. Use “Open folder” instead.`);
  await fs.mkdir(p, { recursive: true });
  return switchVault(p);
});

// Folder browser for "Open folder as vault" (a web page can't show a native picker).
app.get('/api/fs/list', async (req) => {
  const dir = home(req.query.path || '~');
  const ents = await fs.readdir(dir, { withFileTypes: true }).catch(() => { throw files.httpError(400, `Can't read ${dir}`); });
  const dirs = await Promise.all(ents.filter((e) => e.isDirectory() && !e.name.startsWith('.')).map(async (e) => ({
    name: e.name, path: path.join(dir, e.name),
    isVault: !!(await fs.stat(path.join(dir, e.name, '.obsidian')).catch(() => null)),
  })));
  return { path: dir, parent: path.dirname(dir) !== dir ? path.dirname(dir) : null, dirs: dirs.sort((a, b) => a.name.localeCompare(b.name)) };
});

app.get('/api/index/status', async () => ({ ...indexStatus, overview: overviewStatus.running }));
app.post('/api/index', async () => { await reindex(); return indexStatus; });

// ---------- layout
app.get('/api/notebooks', async () => {
  const meta = await loadMeta();
  return (await listNotebooks()).map((nb) => ({ ...nb, meta: meta[nb.path] || {}, sections: nb.sections.map((s) => ({ ...s, meta: meta[s.path] || {} })) }));
});
app.get('/api/meta', async () => loadMeta());
app.put('/api/meta', async (req) => { const m = await setMeta(req.body.path, req.body.patch || {}); emit('tree'); return m; });
app.get('/api/layout', async () => ({ ...(await getSettings()), folders: await topFolders() }));
app.put('/api/layout', async (req) => { const s = await saveSettings({ root: req.body.root }); emit('tree'); reindex(); return s; });
app.get('/api/tree', async () => tree());

// Create a notebook (no parent), a section (parent = notebook) or any folder.
app.post('/api/folder', async (req) => {
  const { root } = await getSettings();
  const rel = await files.createFolder(req.body.parent || root, req.body.name);
  emit('tree');
  return { path: rel };
});
app.get('/api/templates', async () => loadTemplates());
app.put('/api/templates', async (req) => saveTemplates(req.body));
app.post('/api/templates/reset', async () => resetTemplates());

app.post('/api/topic', async (req) => {
  let rel;
  const tpl = req.body.template && (await loadTemplates()).find((t) => t.id === req.body.template);
  if (tpl) {
    const name = files.cleanName(req.body.name);
    const nbs = await listNotebooks();
    const nb = nbs.find((n) => req.body.folder === n.path || req.body.folder.startsWith(n.path + '/'));
    const sec = nb?.sections.find((s) => req.body.folder === s.path || req.body.folder.startsWith(s.path + '/'));
    const now = new Date();
    rel = await files.createNote(req.body.folder, name, fill(tpl.body, {
      title: name, date: now.toISOString().slice(0, 10), time: now.toTimeString().slice(0, 5), notebook: nb?.name || '', section: sec?.name || nb?.name || '',
    }));
  } else rel = await files.createTopic(req.body.folder, req.body.name);
  emit('tree');
  return { path: rel };
});

// Move a note or folder from `from` to `to`, rewriting [[links]] across the vault and re-keying
// app-side data (details, flashcard decks). A topic's sidecar sources folder moves with it.
async function relocate(from, to) {
  const isDir = (await fs.stat(files.safePath(from))).isDirectory();
  if (to === from) return { path: to, moved: [], updated: [] };
  if (isDir && (to + '/').startsWith(from + '/')) throw files.httpError(400, "Can't move a folder into itself");
  await links.build();
  const moved = isDir
    ? links.allFiles().filter((p) => p.startsWith(from + '/')).map((p) => [p, to + p.slice(from.length)])
    : [[from, to]];
  const sidecarFrom = !isDir && path.join(path.dirname(from), 'sources', path.basename(from, '.md'));
  const hasSidecar = sidecarFrom && (await fs.stat(files.safePath(sidecarFrom)).catch(() => null));
  await files.move(from, to);
  if (hasSidecar) {
    const sidecarTo = path.join(path.dirname(to), 'sources', path.basename(to, '.md'));
    links.allFiles().filter((p) => p.startsWith(sidecarFrom + '/')).forEach((p) => moved.push([p, sidecarTo + p.slice(sidecarFrom.length)]));
    await files.move(sidecarFrom, sidecarTo);
    await fs.rmdir(files.safePath(path.dirname(sidecarFrom))).catch(() => {}); // drop an emptied sources/ folder
  }
  const updated = await links.rewriteLinks(moved);
  if (isDir) await rekeyMeta(from, to);
  await rekeyCards(isDir ? [[from, to], ...moved] : moved);
  emit('tree');
  reindex();
  return { path: to, from, isDir, moved, updated };
}

app.post('/api/rename', async (req) => {
  const from = req.body.path;
  const isDir = (await fs.stat(files.safePath(from))).isDirectory();
  const name = files.cleanName(req.body.name);
  if (!name) throw files.httpError(400, 'Name required');
  return relocate(from, path.join(path.dirname(from).replace(/^\.$/, ''), isDir ? name : `${name}${path.extname(from) || '.md'}`));
});

app.post('/api/move', async (req) => {
  const from = req.body.path;
  const toFolder = String(req.body.toFolder ?? '').replace(/\/+$/, '');
  if (path.dirname(from).replace(/^\.$/, '') === toFolder) return { path: from, moved: [], updated: [] };
  return relocate(from, path.join(toFolder, path.basename(from)));
});

app.post('/api/trash', async (req) => {
  const to = await files.trash(req.body.path);
  await dropMeta(req.body.path);
  emit('tree');
  reindex();
  return { trashed: to };
});

// Flat list of notes + headings (+ tags) for ⌘K.
app.get('/api/palette', async () => {
  const table = await getTable();
  const rows = table ? await table.query().select(['path', 'note', 'heading']).toArray() : [];
  const items = new Map();
  for (const p of links.allNotes()) items.set(p, { path: p, note: path.basename(p, '.md'), heading: '', tags: links.tagsOf(p) });
  for (const r of rows) {
    if (r.heading && r.heading !== r.note) items.set(r.path + '#' + r.heading, { path: r.path, note: r.note, heading: r.heading });
  }
  return [...items.values()];
});
app.get('/api/tags', async () => links.allTags());

// ---------- notes
app.get('/api/note', async (req) => {
  const rel = req.query.path;
  const { content, mtime } = await files.readNote(rel);
  const outline = [...content.replace(/```[\s\S]*?```/g, '').matchAll(/^(#{1,6})\s+(.+)$/gm)].map((m) => ({ level: m[1].length, heading: m[2].trim() }));
  const l = links.linksOf(rel);
  const visible = (ps) => ps.filter((p) => !p.split('/').some((s) => SKIP_DIRS.has(s)));
  return {
    path: rel, content, mtime, outline,
    links: visible(l.links), unresolved: l.unresolved, backlinks: visible(links.backlinksOf(rel)),
    tags: links.tagsOf(rel), properties: links.frontmatterOf(rel),
  };
});

app.put('/api/note', async (req) => {
  const mtime = await files.saveNote(req.body.path, req.body.content, req.body.baseMtime);
  return { mtime };
});

app.get('/api/note/mentions', async (req) => links.unlinkedMentions(req.query.path));
app.post('/api/note/link-mention', async (req) => { await links.linkMention(req.body.in, req.body.line, req.body.target); return { ok: true }; });
app.post('/api/note/suggest-links', async (req) => study.suggestLinks(req.body.path, req.body.notebook));
// ⌘K full text: BM25 index first (fast on big vaults), falling back to a file scan.
app.get('/api/search', async (req) => {
  const q = String(req.query.q || '').trim();
  if (!q || q.startsWith('#')) return links.search(q);
  const { ftsQuery } = await import('./indexer.js');
  const table = await getTable();
  try {
    const rows = await table.query().fullTextSearch(ftsQuery(q) || q).where("NOT starts_with(path, '_') AND path NOT LIKE '%/_studio/%'").limit(40).toArray();
    const seen = new Set();
    const out = [];
    const needle = q.toLowerCase().split(/\s+/)[0];
    for (const r of rows) {
      if (seen.has(r.path)) continue;
      seen.add(r.path);
      const line = r.text.split('\n').find((l) => l.toLowerCase().includes(needle)) || r.text.split('\n').find((l) => l.trim() && !l.startsWith('#')) || '';
      out.push({ path: r.path, line: 0, heading: r.heading, text: line.trim().slice(0, 200) });
    }
    if (out.length) return out;
  } catch {}
  return links.search(q);
});
app.get('/api/note/related', async (req) => relatedNotes(req.query.path, { prefix: req.query.notebook }));
// Notebook graph: link edges, plus optional "semantically similar" edges from embeddings.
app.get('/api/graph', async (req) => {
  const g = links.graph(req.query.notebook);
  g.nodes = g.nodes.filter((p) => !p.split('/').some((s) => SKIP_DIRS.has(s)));
  const inGraph = new Set(g.nodes);
  g.edges = g.edges.filter((e) => inGraph.has(e.source) && inGraph.has(e.target));
  g.similar = [];
  if (req.query.similar) {
    const seen = new Set(g.edges.map((e) => [e.source, e.target].sort().join('|')));
    for (const p of g.nodes) {
      for (const r of await relatedNotes(p, { prefix: req.query.notebook, limit: 2 })) {
        const key = [p, r.path].sort().join('|');
        if (r.score >= 0.55 && inGraph.has(r.path) && !seen.has(key)) { seen.add(key); g.similar.push({ source: p, target: r.path, score: r.score }); }
      }
    }
  }
  return g;
});
app.get('/api/resolve', async (req) => ({ path: links.resolve(req.query.note, req.query.from || '') || (await pathForNote(req.query.note)) }));

app.post('/api/attachment', async (req) => {
  const part = await req.file();
  const r = await files.saveAttachment(req.query.note, part.filename, await part.toBuffer());
  await links.build();
  return r;
});

// ---------- chat: newline-delimited JSON {token}… {sources} {followups} {done}
app.post('/api/chat', async (req, reply) => {
  const { question, history } = req.body;
  reply.raw.writeHead(200, { 'content-type': 'application/x-ndjson' });
  const send = (o) => reply.raw.write(JSON.stringify(o) + '\n');
  try {
    await indexing;
    const { sources, answer } = await chat({ question, scope: scopeOf(req.body), history, onToken: (token) => send({ token }) });
    send({ sources });
    send({ followups: await followUps(question, answer) });
  } catch (e) { send({ error: e.message }); }
  send({ done: true });
  reply.raw.end();
});

// Short title for a chat or a saved answer.
app.post('/api/chat/title', async (req) => {
  const { generate } = await import('./llm.js');
  const t = await generate(`Give a short, specific title (max 6 words, no quotes, no trailing period) for this Q&A.\nQ: ${String(req.body.question).slice(0, 500)}\nA: ${String(req.body.answer || '').slice(0, 1200)}\nTitle:`);
  return { title: t.split('\n')[0].replace(/^["'#*\s]+|["'*.\s]+$/g, '').slice(0, 80) || 'Untitled' };
});

// Save a chat answer as a new topic, or append it to an existing one.
app.post('/api/topic/from-answer', async (req) => {
  const { folder, title, question, answer, sources = [], appendTo } = req.body;
  const cites = [...new Set(sources.filter((s) => s.kind !== 'overview').map((s) => `[[${s.note}${s.heading ? '#' + s.heading : ''}]]`))];
  const block = `> **Q:** ${String(question).replace(/\n/g, '\n> ')}\n\n${String(answer).trim()}\n${cites.length ? `\n**Sources:** ${cites.join(' · ')}\n` : ''}`;
  let rel;
  if (appendTo) {
    const { content } = await files.readNote(appendTo);
    await files.writeNote(appendTo, `${content.replace(/\s*$/, '')}\n\n## ${title}\n\n${block}`);
    rel = appendTo;
  } else {
    rel = await files.createNote(folder, title, `---\ncreated: ${new Date().toISOString()}\nfrom: chat\n---\n\n# ${files.cleanName(title)}\n\n${block}`);
  }
  emit('tree');
  return { path: rel };
});

// ---------- studio
app.post('/api/brief', async (req) => brief(scopeOf(req.body), req.body.type));
app.post('/api/study/flashcards', async (req) => study.flashcards(scopeOf(req.body), req.body.count));
app.post('/api/study/deck', async (req) => study.deck(scopeOf(req.body)));
app.post('/api/study/review', async (req) => study.review(req.body.id, req.body.grade));
app.post('/api/study/quiz', async (req) => study.quiz(scopeOf(req.body), req.body));
app.post('/api/study/mindmap', async (req) => study.mindmap(scopeOf(req.body)));
app.post('/api/study/explain', async (req) => study.explainBack(scopeOf(req.body), req.body.subject, req.body.explanation));
app.post('/api/study/card', async (req) => study.addCard(scopeOf(req.body), req.body.card));
app.post('/api/study/drill', async (req) => study.drill(scopeOf(req.body), req.body.concepts || [], req.body.count));
app.post('/api/progress', async (req) => { await progress.logEvent(req.body); return { ok: true }; });
app.get('/api/progress/summary', async (req) => progress.summary({ notebook: req.query.notebook || undefined }));
app.get('/api/progress/due', async (req) => progress.dueCards({ notebook: req.query.notebook || undefined }));
app.post('/api/assist', async (req) => study.assist(req.body.action, req.body.text, scopeOf(req.body)));

const jobs = {};
const jobKey = (b) => `${vault.path}|${b.topic || baseOf(scopeOf(b))}`;
app.post('/api/audio', async (req) => {
  const key = jobKey(req.body);
  jobs[key] = { status: 'Starting…' };
  audioOverview(scopeOf(req.body), (s) => (jobs[key].status = s))
    .then((r) => (jobs[key] = { status: 'done', ...r }))
    .catch((e) => (jobs[key] = { status: 'error', error: e.message }));
  return jobs[key];
});
app.post('/api/audio/status', async (req) => jobs[jobKey(req.body)] || { status: 'idle' });

// ---------- sources (saved beside their owner; topic sources go to a sidecar folder, the note is untouched)
app.post('/api/source/url', async (req) => ({ path: await addUrl(sourcesDirFor(scopeOf(req.body)), req.body.url) }));
// Any supported file: PDF (OCR for scans), EPUB, images (OCR), .txt/.md/.html.
async function sourceFiles(req) {
  const saved = [], failed = [];
  for await (const part of req.files()) {
    try { saved.push(await addFile(sourcesDirFor(scopeOf(req.query)), part.filename, await part.toBuffer())); }
    catch (e) { failed.push({ name: part.filename, error: e.message }); }
  }
  if (!saved.length && failed.length) throw files.httpError(400, failed.map((f) => `${f.name}: ${f.error}`).join('; '));
  return { paths: saved, failed };
}
app.post('/api/source/file', sourceFiles);
// Remove a source: the note goes to .trash, along with its saved original (if it lives in a sources/ folder).
app.post('/api/source/remove', async (req) => {
  const rel = req.body.path;
  if (!rel.split('/').includes('sources')) throw files.httpError(400, 'Not a source');
  const { content } = await files.readNote(rel);
  const original = /^original:\s*"?([^"\n]+)"?/m.exec(content)?.[1];
  const trashed = [await files.trash(rel)];
  if (original && original.split('/').includes('sources') && (await fs.stat(files.safePath(original)).catch(() => null))) {
    trashed.push(await files.trash(original));
    await fs.rmdir(files.safePath(path.dirname(original))).catch(() => {}); // drop an emptied attachments/ folder
  }
  await fs.rmdir(files.safePath(path.dirname(rel))).catch(() => {});     // and an emptied sources/ (or sidecar) folder
  emit('tree');
  reindex();
  return { trashed };
});
app.post('/api/source/reprocess', async (req) => { const p = await reprocess(req.body.path); emit('tree'); return { path: p }; });
app.post('/api/source/pdf', sourceFiles);

const MIME = { '.wav': 'audio/wav', '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp', '.svg': 'image/svg+xml', '.pdf': 'application/pdf', '.md': 'text/markdown' };
app.get('/api/file', async (req, reply) => {
  const full = files.safePath(req.query.path);
  return reply.type(MIME[path.extname(full).toLowerCase()] || 'application/octet-stream').send(await fs.readFile(full));
});

await watch();
reindex();
const PORT = Number(process.env.PORT) || 3737;
await app.listen({ port: PORT });
console.log(`vault-lm on http://localhost:${PORT}  (vault: ${vault.path})`);
