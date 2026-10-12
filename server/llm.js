import path from 'node:path';
import { STUDIO } from './config.js';
import * as models from './models.js';
import { retrieve, scopeText } from './retrieve.js';
import { baseOf } from './layout.js';
import { writeNote } from './files.js';
import { goalsContext } from './meta.js';


export const cite = (c) => `[[${c.note}${c.heading ? '#' + c.heading : ''}]]`;

const SYSTEM = `You answer questions using ONLY the provided sources from the user's notes.
Answer in your own words. Don't copy source chunks or their headings into the answer.
After each claim, cite the source using its exact [[Note#Heading]] tag.
Sources marked (from: X) come from elsewhere in the same notebook, outside the user's current selection — mention when you rely on them.
(overview) is a high-level summary of the whole notebook: use it only as background context, and prefer the other sources for facts.
(notebook goals) describes what the user is working toward. Use it to decide what matters and how to frame the answer, but never cite it as a fact.
If the sources don't contain the answer, say so plainly.`;

// Streams answer tokens via onToken; returns { sources }.
export async function chat({ question, scope, history = [], onToken }) {
  const sources = await retrieve(question, scope);
  const label = (c) => (c.kind === 'overview' ? ' (overview)' : c.kind === 'notebook' ? ` (from: ${c.origin})` : '');
  const goals = await goalsContext(scope);
  const ctx = (goals ? `--- (notebook goals)\n${goals}\n\n` : '') + sources.map((c) => `--- ${cite(c)}${label(c)}\n${c.text}`).join('\n\n');
  const messages = [
    { role: 'system', content: SYSTEM },
    ...history.slice(-6),
    { role: 'user', content: `Sources:\n${ctx || '(none)'}\n\nQuestion: ${question}` },
  ];
  const answer = await models.chat(messages, { onToken });
  return { sources, answer };
}

export async function followUps(question, answer) {
  const r = await generateJSON(`Given this Q&A from a study session, suggest 3 short follow-up questions the student might ask next.
Return JSON: {"questions": ["...", "...", "..."]}

Q: ${question}
A: ${answer.slice(0, 2000)}`).catch(() => null);
  return Array.isArray(r?.questions) ? r.questions.slice(0, 3).map(String) : [];
}

export async function generate(prompt) {
  return (await models.chat([{ role: 'user', content: prompt }])).trim();
}

// JSON-mode generation with one retry; `check` validates the parsed object.
export async function generateJSON(prompt, check = () => true) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const text = await models.chat(
      [{ role: 'system', content: 'Respond with valid JSON only.' }, { role: 'user', content: prompt }],
      { json: true, temperature: attempt ? 0.2 : 0.6 },
    );
    try {
      const obj = JSON.parse(text);
      if (check(obj)) return obj;
    } catch {}
  }
  throw new Error('The model did not return valid output — try again');
}

export async function textFor(scope, max) {
  const text = await scopeText(scope, max);
  if (!text) throw new Error('Nothing indexed in this scope yet');
  // Studio tools all build on this, so learning goals steer every generated quiz, card, guide and map.
  const goals = await goalsContext(scope);
  return goals ? `[Learning goals: prioritize material that serves these. They are context, not notes to quote.]\n${goals}\n\n${text}` : text;
}

// Where generated files go: <notebook>/_studio/<name>.md (note scope gets a prefixed name).
export function studioPath(scope, name) {
  const base = baseOf(scope);
  const prefix = scope.topic ? `${path.basename(scope.topic, '.md')} - ` : '';
  return path.join(base, STUDIO, `${prefix}${name}.md`);
}

export const BRIEFS = {
  summary: 'Write a concise briefing document: an overview paragraph, then key themes as headed sections with bullet points.',
  faq: 'Write an FAQ: 8-12 likely questions with clear answers.',
  'study-guide': 'Write a study guide: key terms with definitions, 5 short-answer questions with answers, and 3 essay prompts.',
  timeline: 'Write a chronological timeline of events/dates mentioned, as a bulleted list, followed by a short cast of key people/things.',
};

export async function brief(scope, type) {
  const text = await textFor(scope);
  const body = await generate(`${BRIEFS[type]}\nBase it only on these notes, citing [[Note#Heading]] tags where useful. Output Markdown only.\n\nNotes:\n${text}`);
  const p = studioPath(scope, type);
  await writeNote(p, `---\ngenerated: ${new Date().toISOString()}\ntype: ${type}\n---\n\n${body}\n`);
  return { path: p, body };
}

export async function podcastScript(scope) {
  const text = await textFor(scope, 16000);
  const raw = await generate(`Write a lively 3-5 minute podcast conversation between two hosts, A and B, discussing these notes. A explains, B asks curious questions and reacts. Each line must start with "A:" or "B:". No stage directions, no markdown.\n\nNotes:\n${text}`);
  return raw.split('\n').map((l) => l.match(/^\**\s*([AB])\s*\**:\s*(.+)/)).filter(Boolean)
    .map((m) => ({ speaker: m[1], text: m[2].replace(/\*/g, '').trim() }));
}
