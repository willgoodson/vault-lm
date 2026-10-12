// Vault layout: Notebook (folder under the per-vault root) → Section (subfolder) → Topic (.md file).
// Sources live beside their owner: <nb>/sources/, <section>/sources/, <dir>/sources/<Topic>/ (topic sidecar).
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';
import { SKIP_DIRS } from './config.js';
import { esc } from './indexer.js';

export const NON_CONTENT = new Set([...SKIP_DIRS, 'sources', 'attachments']);
const settingsFile = () => path.join(vault.data, 'settings.json');
const readJSON = async (f) => JSON.parse(await fs.readFile(f, 'utf8').catch(() => '{}'));

export async function getSettings() { return { root: '', ...(await readJSON(settingsFile())) }; }
export async function saveSettings(patch) {
  const s = { ...(await getSettings()), ...patch };
  s.root = String(s.root || '').replace(/^\/+|\/+$/g, '');
  await fs.mkdir(vault.data, { recursive: true });
  await fs.writeFile(settingsFile(), JSON.stringify(s, null, 2));
  return s;
}

const entries = async (rel) => (await fs.readdir(path.join(vault.path, rel), { withFileTypes: true }).catch(() => []))
  .filter((e) => !e.name.startsWith('.'))
  .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));

export const topFolders = async () => (await entries('')).filter((e) => e.isDirectory()).map((e) => e.name);

const md = (name, rel) => ({ name: name.replace(/\.md$/, ''), path: rel });

async function mdFiles(rel, recursive) {
  const out = [];
  for (const e of await entries(rel)) {
    const p = rel ? `${rel}/${e.name}` : e.name;
    if (e.isDirectory()) { if (recursive && !NON_CONTENT.has(e.name)) out.push(...(await mdFiles(p, true))); }
    else if (e.name.endsWith('.md')) out.push(p);
  }
  return out;
}

// Files directly in a sources/ folder (not topic sidecars, which are its subfolders).
const sourcesIn = async (folder) => (await mdFiles(`${folder}/sources`, false)).map((p) => md(path.basename(p), p));

async function topicsIn(folder, recursive) {
  const out = [];
  for (const p of await mdFiles(folder, recursive)) {
    const rel = path.relative(folder, p);
    const sidecar = topicSourcesDir(p);
    out.push({
      name: rel.replace(/\.md$/, ''),
      path: p,
      sources: (await mdFiles(sidecar, true)).map((s) => md(path.basename(s), s)),
    });
  }
  return out;
}

export async function listNotebooks() {
  const { root } = await getSettings();
  const out = [];
  for (const e of await entries(root)) {
    if (!e.isDirectory() || SKIP_DIRS.has(e.name)) continue;
    const nb = root ? `${root}/${e.name}` : e.name;
    const sections = [];
    for (const s of await entries(nb)) {
      if (!s.isDirectory() || NON_CONTENT.has(s.name)) continue;
      const sp = `${nb}/${s.name}`;
      sections.push({ name: s.name, path: sp, topics: await topicsIn(sp, true), sources: await sourcesIn(sp) });
    }
    out.push({ name: e.name, path: nb, topics: await topicsIn(nb, false), sections, sources: await sourcesIn(nb) });
  }
  return out;
}

export const topicCount = (nb) => nb.topics.length + nb.sections.reduce((n, s) => n + s.topics.length, 0);

// ---- scope helpers. scope = { notebook, section?, topic?, mode? }
export const topicSourcesDir = (topic) => {
  const dir = path.dirname(topic).replace(/^\.$/, '');
  return path.join(dir, 'sources', path.basename(topic, '.md'));
};

// Folder that a scope's generated output lives in.
export const baseOf = (s) => (s.topic ? path.dirname(s.topic).replace(/^\.$/, '') : s.section || s.notebook || '');

// Where "Add source" saves for a scope.
export const sourcesDirFor = (s) => (s.topic ? topicSourcesDir(s.topic) : path.join(s.section || s.notebook, 'sources'));

const under = (folder) => `starts_with(path, '${esc(folder)}/')`;
// LanceDB filter for a scope's own content (topic = the note + its sidecar sources).
export function targetWhere(s) {
  if (s.topic) return `(path = '${esc(s.topic)}' OR ${under(topicSourcesDir(s.topic))})`;
  return under(s.section || s.notebook);
}
export const notebookWhere = (s) => under(s.notebook);

// Human label for where a path sits inside a notebook.
export function originOf(nbTree, p) {
  const sec = nbTree?.sections.find((s) => p.startsWith(s.path + '/'));
  return sec ? sec.name : nbTree?.name || 'notebook';
}
