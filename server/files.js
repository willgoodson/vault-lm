// All vault filesystem access. The vault stays Obsidian-compatible: plain Markdown,
// nothing is ever written to .obsidian/, deletes go to .trash/ (Obsidian's "vault trash").
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';

export const httpError = (code, msg) => Object.assign(new Error(msg), { statusCode: code });

export function safePath(rel) {
  const full = path.resolve(vault.path, String(rel ?? ''));
  if (full !== vault.path && !full.startsWith(vault.path + path.sep)) throw httpError(403, 'Outside vault');
  const parts = path.relative(vault.path, full).split(path.sep);
  if (parts[0] === '.obsidian') throw httpError(403, 'Vault-LM never touches .obsidian/');
  return full;
}

export const cleanName = (n) => String(n || '').replace(/[\\/:*?"<>|#^[\]]/g, '').replace(/\s+/g, ' ').trim();
const mtimeOf = async (full) => (await fs.stat(full)).mtimeMs;

export async function readNote(rel) {
  const full = safePath(rel);
  const [content, mtime] = await Promise.all([fs.readFile(full, 'utf8'), mtimeOf(full)]);
  return { content, mtime };
}

export async function writeNote(rel, content) {
  const full = safePath(rel);
  await fs.mkdir(path.dirname(full), { recursive: true });
  await fs.writeFile(full, content);
  return mtimeOf(full);
}

// Editor save with optimistic concurrency: refuse if the file changed on disk since it was loaded.
export async function saveNote(rel, content, baseMtime) {
  const full = safePath(rel);
  const current = await mtimeOf(full).catch(() => null);
  if (current !== null && baseMtime != null && Math.abs(current - baseMtime) > 1) {
    const err = httpError(409, 'Changed outside Vault-LM');
    err.current = await fs.readFile(full, 'utf8');
    err.mtime = current;
    throw err;
  }
  return writeNote(rel, content);
}

const exists = (full) => fs.stat(full).then(() => true, () => false);

// "Name.md", "Name 1.md", … — never overwrite.
async function freeName(dirFull, base, ext) {
  for (let i = 0; ; i++) {
    const name = `${base}${i ? ` ${i}` : ''}${ext}`;
    if (!(await exists(path.join(dirFull, name)))) return name;
  }
}

export async function createTopic(folder, name) {
  name = cleanName(name);
  if (!name) throw httpError(400, 'Name required');
  const dirFull = safePath(folder);
  await fs.mkdir(dirFull, { recursive: true });
  const file = await freeName(dirFull, name, '.md');
  const rel = path.join(folder, file);
  await writeNote(rel, `# ${path.basename(file, '.md')}\n\n`);
  return rel;
}

// New note with given content (unique name, never overwrites).
export async function createNote(folder, name, content) {
  name = cleanName(name);
  if (!name) throw httpError(400, 'Name required');
  const dirFull = safePath(folder || '.');
  await fs.mkdir(dirFull, { recursive: true });
  const file = await freeName(dirFull, name, '.md');
  const rel = path.join(folder || '', file);
  await writeNote(rel, content);
  return rel;
}

export async function createFolder(parent, name) {
  name = cleanName(name);
  if (!name) throw httpError(400, 'Name required');
  const rel = parent ? path.join(parent, name) : name;
  const full = safePath(rel);
  if (await exists(full)) throw httpError(409, `“${name}” already exists`);
  await fs.mkdir(full, { recursive: true });
  return rel;
}

export async function trash(rel) {
  const full = safePath(rel);
  if (full === vault.path) throw httpError(400, 'Cannot delete the vault');
  const trashDir = path.join(vault.path, '.trash');
  await fs.mkdir(trashDir, { recursive: true });
  const ext = path.extname(full);
  const dest = path.join(trashDir, await freeName(trashDir, path.basename(full, ext), ext));
  await fs.rename(full, dest);
  return path.relative(vault.path, dest);
}

export async function move(rel, newRel) {
  const from = safePath(rel), to = safePath(newRel);
  if (await exists(to)) throw httpError(409, `“${path.basename(newRel)}” already exists`);
  await fs.mkdir(path.dirname(to), { recursive: true });
  await fs.rename(from, to);
}

// Obsidian's attachment setting (read-only). Without one, images go in an "attachments" folder next to the note.
export async function attachmentDir(noteRel) {
  const cfg = JSON.parse(await fs.readFile(path.join(vault.path, '.obsidian/app.json'), 'utf8').catch(() => '{}'));
  const setting = cfg.attachmentFolderPath;
  const noteDir = path.dirname(noteRel).replace(/^\.$/, '');
  if (setting === undefined) return path.join(noteDir, 'attachments');
  if (setting === '/' || setting === '') return '';
  if (setting === './') return noteDir;
  if (setting.startsWith('./')) return path.join(noteDir, setting.slice(2));
  return setting;
}

export async function saveAttachment(noteRel, filename, buffer) {
  const dir = await attachmentDir(noteRel);
  const dirFull = safePath(dir || '.');
  await fs.mkdir(dirFull, { recursive: true });
  const ext = path.extname(filename) || '.png';
  const base = cleanName(path.basename(filename, ext)) || `Pasted image ${new Date().toISOString().replace(/[:.]/g, '-')}`;
  const name = await freeName(dirFull, base, ext);
  await fs.writeFile(path.join(dirFull, name), buffer);
  return { path: path.join(dir, name), name };
}
