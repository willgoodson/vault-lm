// Auto-maintained "Notebook Overview": a short high-level summary of a whole notebook,
// used as background context when chatting in Focused mode on one section.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { STUDIO } from './config.js';
import { vault } from './vaults.js';
import { getTable, esc } from './indexer.js';
import { listNotebooks, topicCount } from './layout.js';
import { writeNote } from './files.js';
import { goalsContext } from './meta.js';

const NAME = 'Notebook Overview';
const fileFor = (nb) => path.join(nb, STUDIO, `${NAME}.md`);
export const overviewStatus = { running: null, queue: [] };

async function signature(nb) {
  const table = await getTable();
  if (!table) return null;
  const rows = await table.query().where(`starts_with(path, '${esc(nb)}/')`).select(['path', 'mtime']).toArray();
  if (!rows.length) return null;
  const uniq = [...new Set(rows.map((r) => `${r.path}:${r.mtime}`))].sort();
  // Editing the notebook's description/goals also refreshes the overview.
  const goals = await goalsContext({ notebook: nb });
  return crypto.createHash('sha1').update(uniq.join('\n') + '\n' + goals).digest('hex').slice(0, 16);
}

async function read(nb) {
  const raw = await fs.readFile(path.join(vault.path, fileFor(nb)), 'utf8').catch(() => null);
  if (!raw) return null;
  const sig = raw.match(/^signature:\s*(\w+)/m)?.[1];
  return { sig, text: raw.replace(/^---\n[\s\S]*?\n---\n/, '').trim() };
}

export async function getOverview(nb) {
  const o = await read(nb);
  return o?.text ? { note: NAME, path: fileFor(nb), text: o.text.slice(0, 2500) } : null;
}

async function generate(nb) {
  // Lazy imports avoid a cycle (llm → retrieve → overview).
  const { generate: gen } = await import('./llm.js');
  const { scopeText } = await import('./retrieve.js');
  const info = (await listNotebooks()).find((n) => n.path === nb);
  const text = await scopeText({ notebook: nb }, 20000);
  if (!text) return;
  const sections = info?.sections.map((s) => s.name).join(', ') || '(none)';
  const body = await gen(`Write a high-level overview of this notebook titled "${info?.name || nb}" in 120-220 words of Markdown.
Start with 2-3 sentences on its overall focus and goal. Then a "Sections" list with one line per section saying what it covers.
Sections: ${sections}
${(await goalsContext({ notebook: nb })) ? `The user describes this notebook like this (use it to frame the overview, not as a source of facts):\n${await goalsContext({ notebook: nb })}\n` : ''}Be factual and only use the notes below.

Notes:\n${text}`);
  return body;
}

// After indexing: regenerate overviews for notebooks (with sections) whose content changed.
export async function refreshOverviews(log = console.log) {
  if (overviewStatus.running) return;
  const target = vault.data;
  for (const nb of await listNotebooks()) {
    if (topicCount(nb) < 2 || vault.data !== target) continue;
    const sig = await signature(nb.path);
    if (!sig || (await read(nb.path))?.sig === sig) continue;
    overviewStatus.running = nb.path;
    try {
      const body = await generate(nb.path);
      if (body && vault.data === target) {
        await writeNote(fileFor(nb.path), `---\nsignature: ${sig}\ngenerated: ${new Date().toISOString()}\n---\n\n${body}\n`);
        log(`overview updated: ${nb.path}`);
      }
    } catch (e) { log(`overview failed for ${nb.path}: ${e.message}`); }
    overviewStatus.running = null;
  }
}
