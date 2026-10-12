// App-side details for notebooks and sections (description, goals, status, due, color).
// Kept in Vault-LM's data folder, keyed by vault-relative folder path — the vault itself stays pure notes.
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';

const file = () => path.join(vault.data, 'meta.json');
const FIELDS = ['description', 'goals', 'status', 'due', 'color'];

export async function loadMeta() { return JSON.parse(await fs.readFile(file(), 'utf8').catch(() => '{}')); }
async function save(m) { await fs.mkdir(vault.data, { recursive: true }); await fs.writeFile(file(), JSON.stringify(m, null, 2)); }

export async function setMeta(p, patch) {
  const m = await loadMeta();
  const cur = { ...(m[p] || {}) };
  for (const k of FIELDS) {
    if (!(k in patch)) continue;
    let v = patch[k];
    if (k === 'goals') v = [v].flat().map((g) => String(g).trim()).filter(Boolean);
    else v = v == null ? '' : String(v).trim();
    if ((Array.isArray(v) && !v.length) || v === '') delete cur[k]; else cur[k] = v;
  }
  if (Object.keys(cur).length) m[p] = cur; else delete m[p];
  await save(m);
  return cur;
}

// Move/rename: re-key everything at or under `from` to `to`.
export async function rekeyMeta(from, to) {
  const m = await loadMeta();
  let changed = false;
  for (const k of Object.keys(m)) {
    if (k === from || k.startsWith(from + '/')) { m[to + k.slice(from.length)] = m[k]; delete m[k]; changed = true; }
  }
  if (changed) await save(m);
}
export async function dropMeta(p) {
  const m = await loadMeta();
  for (const k of Object.keys(m)) if (k === p || k.startsWith(p + '/')) delete m[k];
  await save(m);
}

// Text block describing what the user wants from this notebook/section, for prompts.
export async function goalsContext(scope) {
  const m = await loadMeta();
  const parts = [];
  for (const [label, p] of [['Notebook', scope.notebook], ['Section', scope.section]]) {
    const d = p && m[p];
    if (!d || (!d.description && !d.goals?.length)) continue;
    const name = p.split('/').pop();
    parts.push(`${label} "${name}"${d.description ? `: ${d.description}` : ''}${d.goals?.length ? `\nGoals:\n${d.goals.map((g) => `- ${g}`).join('\n')}` : ''}`);
  }
  return parts.join('\n\n');
}
