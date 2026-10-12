// Topic templates, kept app-side per vault (data/vaults/<slug>/templates.json).
// Variables: {{title}} {{date}} {{time}} {{notebook}} {{section}}
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';

const DEFAULTS = [
  { id: 'lecture', name: 'Lecture note', body: '# {{title}}\n\n**Date:** {{date}} · **Course:** {{notebook}}\n\n## Key ideas\n- \n\n## Details\n\n## Questions I still have\n- \n\n## Summary (in my own words)\n' },
  { id: 'paper', name: 'Paper summary', body: '---\ntype: paper\nauthors: \nyear: \n---\n\n# {{title}}\n\n## Question\n\n## Method\n\n## Findings\n- \n\n## Limitations\n\n## How it connects\n- [[]]\n' },
  { id: 'chapter', name: 'Book chapter', body: '# {{title}}\n\n> Book: {{section}}\n\n## Summary\n\n## Key quotes\n> \n\n## Characters / concepts\n- \n\n## My thoughts\n' },
  { id: 'meeting', name: 'Meeting', body: '# {{title}}\n\n**When:** {{date}} {{time}}\n**Attendees:** \n\n## Agenda\n- \n\n## Notes\n\n## Decisions\n- \n\n## Action items\n- [ ] \n' },
  { id: 'brief', name: 'Project brief', body: '# {{title}}\n\n## Goal\n\n## Why it matters\n\n## Scope\n- In: \n- Out: \n\n## Milestones\n- [ ] \n\n## Risks\n- \n' },
];

const file = () => path.join(vault.data, 'templates.json');
export async function loadTemplates() {
  const raw = await fs.readFile(file(), 'utf8').catch(() => null);
  return raw ? JSON.parse(raw) : DEFAULTS;
}
export async function saveTemplates(list) {
  const clean = (Array.isArray(list) ? list : []).filter((t) => t?.name).map((t) => ({
    id: String(t.id || t.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')).slice(0, 40), name: String(t.name).slice(0, 60), body: String(t.body || ''),
  }));
  await fs.mkdir(vault.data, { recursive: true });
  await fs.writeFile(file(), JSON.stringify(clean, null, 2));
  return clean;
}
export const resetTemplates = () => saveTemplates(DEFAULTS);

export function fill(body, vars) {
  return body.replace(/\{\{\s*(\w+)\s*\}\}/g, (m, k) => (vars[k] ?? m));
}
