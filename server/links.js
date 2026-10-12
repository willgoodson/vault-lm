// Link index: Obsidian-style resolution of [[wikilinks]], ![[embeds]] and [md](links),
// plus tags and frontmatter. Rebuilt incrementally (by mtime) after each index run.
import fs from 'node:fs/promises';
import path from 'node:path';
import matter from 'gray-matter';
import { vault } from './vaults.js';
import { writeNote } from './files.js';

const WIKI = /(!?)\[\[([^\]|#^]*)(#[^\]|^]*)?(\^[^\]|]*)?(\|[^\]]*)?\]\]/g;
const MDLINK = /\[[^\]]*\]\((?!https?:|mailto:|obsidian:)([^)\s]+?\.md)(#[^)]*)?\)/g;
const TAG = /(?:^|[\s(])#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/gu;

const idx = { key: '', files: new Map(), all: [] }; // files: rel -> { mtime, links, tags, fm }

async function walk(dir, out = []) {
  for (const e of await fs.readdir(dir, { withFileTypes: true }).catch(() => [])) {
    if (e.name.startsWith('.')) continue;
    const full = path.join(dir, e.name);
    if (e.isDirectory()) await walk(full, out);
    else out.push(path.relative(vault.path, full));
  }
  return out;
}

const stripCode = (s) => s.replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]*`/g, '');

export function parse(content) {
  let fm = {};
  let body = content;
  try { const m = matter(content); fm = m.data || {}; body = m.content; } catch {}
  const text = stripCode(body);
  const links = [];
  for (const m of text.matchAll(WIKI)) {
    links.push({ target: m[2].trim(), heading: m[3]?.slice(1) || '', embed: !!m[1], alias: m[5]?.slice(1) || '' });
  }
  for (const m of text.matchAll(MDLINK)) links.push({ target: decodeURIComponent(m[1]), heading: m[2]?.slice(1) || '', embed: false, md: true });
  const tags = new Set([...text.replace(/^#{1,6}\s/gm, '').matchAll(TAG)].map((m) => m[1].toLowerCase()));
  [fm.tags ?? []].flat().filter(Boolean).forEach((t) => tags.add(String(t).replace(/^#/, '').toLowerCase()));
  return { links, tags: [...tags], fm };
}

// Obsidian resolution: path-qualified match first, then unique basename, else shortest path.
export function resolve(target, from = '') {
  if (!target) return from || null; // [[#Heading]] links to self
  const t = target.replace(/^\/+/, '');
  const candidates = /\.\w+$/.test(t) && !t.endsWith('.md') ? [t] : [`${t}.md`, t];
  for (const c of candidates) {
    if (idx.all.includes(c)) return c;
    const rel = path.join(path.dirname(from), c);
    if (idx.all.includes(rel)) return rel;
  }
  const base = path.basename(candidates[0]).toLowerCase();
  const hits = idx.all.filter((p) => p.toLowerCase().endsWith('/' + base) || p.toLowerCase() === base);
  if (!hits.length) return null;
  if (t.includes('/')) {
    const tail = hits.filter((p) => p.toLowerCase().endsWith(candidates[0].toLowerCase()));
    if (tail.length) return tail.sort((a, b) => a.length - b.length)[0];
  }
  return hits.sort((a, b) => a.split('/').length - b.split('/').length || a.length - b.length)[0];
}

export async function build() {
  if (idx.key !== vault.path) { idx.files = new Map(); idx.key = vault.path; }
  idx.all = await walk(vault.path);
  const seen = new Set();
  for (const rel of idx.all) {
    if (!rel.endsWith('.md')) continue;
    seen.add(rel);
    const mtime = (await fs.stat(path.join(vault.path, rel)).catch(() => null))?.mtimeMs;
    if (!mtime || idx.files.get(rel)?.mtime === mtime) continue;
    const content = await fs.readFile(path.join(vault.path, rel), 'utf8').catch(() => '');
    idx.files.set(rel, { mtime, ...parse(content) });
  }
  for (const rel of idx.files.keys()) if (!seen.has(rel)) idx.files.delete(rel);
}

const resolved = (rel) => (idx.files.get(rel)?.links || []).map((l) => ({ ...l, path: resolve(l.target, rel) }));

export function linksOf(rel) {
  const out = resolved(rel);
  return {
    links: [...new Set(out.filter((l) => l.path && l.path !== rel && !l.embed).map((l) => l.path))],
    embeds: [...new Set(out.filter((l) => l.path && l.embed).map((l) => l.path))],
    unresolved: [...new Set(out.filter((l) => !l.path).map((l) => l.target))],
  };
}

export function backlinksOf(rel) {
  const out = [];
  for (const p of idx.files.keys()) if (p !== rel && resolved(p).some((l) => l.path === rel)) out.push(p);
  return out;
}

export const tagsOf = (rel) => idx.files.get(rel)?.tags || [];
export const frontmatterOf = (rel) => idx.files.get(rel)?.fm || {};
export const allNotes = () => [...idx.files.keys()];
export const allFiles = () => idx.all;

export function allTags() {
  const counts = {};
  for (const f of idx.files.values()) for (const t of f.tags) counts[t] = (counts[t] || 0) + 1;
  return Object.entries(counts).map(([tag, count]) => ({ tag, count })).sort((a, b) => b.count - a.count);
}

// Link graph for a set of notes (e.g. a notebook).
export function graph(prefix) {
  const inScope = (p) => p.endsWith('.md') && (!prefix || p.startsWith(prefix + '/'));
  const nodes = [...idx.files.keys()].filter(inScope);
  const edges = [];
  for (const p of nodes) for (const l of linksOf(p).links) if (inScope(l)) edges.push({ source: p, target: l });
  return { nodes, edges };
}

/**
 * After moving files (oldRel -> newRel pairs), rewrite links in every note that pointed at them.
 * Keeps the link short (basename) when that still resolves uniquely, like Obsidian.
 */
export async function rewriteLinks(moves) {
  const before = new Map(); // note -> resolved links before the move
  for (const p of idx.files.keys()) before.set(p, resolved(p));
  const map = new Map(moves);
  await build(); // picks up the moved files
  const changed = [];
  for (const [note, links] of before) {
    const noteNow = map.get(note) || note;
    if (!links.some((l) => l.path && map.has(l.path))) continue;
    const full = path.join(vault.path, noteNow);
    let content = await fs.readFile(full, 'utf8').catch(() => null);
    if (content == null) continue;
    const nameFor = (newPath) => {
      const short = newPath.replace(/\.md$/, '').split('/').pop();
      return resolve(short, noteNow) === newPath ? short : newPath.replace(/\.md$/, '');
    };
    const next = content
      .replace(WIKI, (m, bang, target, heading = '', block = '', alias = '') => {
        const old = resolveIn(before, target.trim(), note);
        return old && map.has(old) ? `${bang}[[${nameFor(map.get(old))}${heading}${block}${alias}]]` : m;
      })
      .replace(MDLINK, (m, target, heading = '') => {
        const old = resolveIn(before, decodeURIComponent(target), note);
        if (!old || !map.has(old)) return m;
        const rel = path.relative(path.dirname(noteNow), map.get(old)).split(path.sep).map(encodeURIComponent).join('/');
        return m.replace(target + heading, rel + heading);
      });
    if (next !== content) { await writeNote(noteNow, next); changed.push(noteNow); }
  }
  await build();
  return changed;
}

// Resolve against the pre-move state recorded in `before`.
function resolveIn(before, target, from) {
  const hit = (before.get(from) || []).find((l) => l.target === target);
  return hit?.path ?? null;
}

// Turn the first plain-text mention of a note on a given line into a [[link]].
export async function linkMention(inPath, lineNo, targetPath) {
  const name = path.basename(targetPath, '.md');
  const full = path.join(vault.path, inPath);
  const lines = (await fs.readFile(full, 'utf8')).split('\n');
  // Mentions are reported against the body (frontmatter stripped), so offset the line.
  const fm = /^---\n[\s\S]*?\n---\n/.exec(lines.join('\n'));
  const offset = fm ? fm[0].split('\n').length - 1 : 0;
  const i = lineNo + offset;
  const re = new RegExp(`(?<![\\[\\w])(${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})(?![\\w\\]])`, 'i');
  if (!re.test(lines[i] ?? '')) throw Object.assign(new Error('Mention not found (the note may have changed)'), { statusCode: 409 });
  const short = resolve(name, inPath) === targetPath ? name : targetPath.replace(/\.md$/, '');
  lines[i] = lines[i].replace(re, (m) => (m === short ? `[[${short}]]` : `[[${short}|${m}]]`));
  await writeNote(inPath, lines.join('\n'));
  await build();
}

// Full-text search over notes: [{path, line, text}] (case-insensitive), or notes with a #tag.
export async function search(q, limit = 40) {
  q = q.trim();
  if (!q) return [];
  if (q.startsWith('#')) {
    const tag = q.slice(1).toLowerCase();
    return [...idx.files.entries()].filter(([, f]) => f.tags.some((t) => t === tag || t.startsWith(tag + '/'))).slice(0, limit).map(([p]) => ({ path: p, line: 0, text: `#${tag}` }));
  }
  const needle = q.toLowerCase();
  const out = [];
  for (const p of idx.files.keys()) {
    if (p.split('/').includes('_studio')) continue;
    const content = await fs.readFile(path.join(vault.path, p), 'utf8').catch(() => '');
    const lines = content.split('\n');
    for (let i = 0; i < lines.length && out.length < limit; i++) {
      if (lines[i].toLowerCase().includes(needle)) { out.push({ path: p, line: i, text: lines[i].trim().slice(0, 200) }); break; }
    }
    if (out.length >= limit) break;
  }
  return out;
}

// Plain-text mentions of a note's name in other notes that don't already link to it.
export async function unlinkedMentions(rel, limit = 30) {
  const name = path.basename(rel, '.md');
  if (name.length < 3) return [];
  const re = new RegExp(`(?<![\\[\\w])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w\\]])`, 'i');
  const back = new Set(backlinksOf(rel));
  const out = [];
  for (const p of idx.files.keys()) {
    if (p === rel || back.has(p) || p.split('/').some((s) => s === '_studio')) continue;
    const content = await fs.readFile(path.join(vault.path, p), 'utf8').catch(() => '');
    const lines = stripCode(content.replace(/^---\n[\s\S]*?\n---\n/, '')).split('\n');
    lines.forEach((line, i) => {
      if (out.length < limit && re.test(line.replace(/\[\[[^\]]*\]\]/g, ''))) out.push({ path: p, line: i, text: line.trim().slice(0, 200) });
    });
  }
  return out;
}
