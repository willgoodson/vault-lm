import { generate, generateJSON, textFor, studioPath } from './llm.js';
import { retrieve } from './retrieve.js';
import { writeNote } from './files.js';
import * as srs from './srs.js';

const CITE_RULE = 'Every "cite" must be one of the exact [[Note#Heading]] tags from the notes.';
const deckName = (scope) => scope.topic || scope.section || scope.notebook;

export async function flashcards(scope, count = 15) {
  const text = await textFor(scope);
  const { cards } = await generateJSON(`Create ${count} high-quality flashcards from these notes for spaced-repetition study.
Fronts are specific questions or terms; backs are concise answers (1-2 sentences). ${CITE_RULE}
Return JSON: {"cards":[{"front":"...","back":"...","cite":"[[Note#Heading]]"}]}

Notes:\n${text}`, (o) => Array.isArray(o.cards) && o.cards.length && o.cards.every((c) => c.front && c.back));
  const clean = cards.map((c) => ({ front: String(c.front).trim(), back: String(c.back).trim(), cite: String(c.cite || '') }));
  await srs.addCards(deckName(scope), clean);
  const p = await mirrorDeck(scope);
  return { path: p, ...(await srs.deck(deckName(scope))) };
}

// Mirror a deck into the vault in the Obsidian Spaced Repetition plugin's format.
async function mirrorDeck(scope) {
  const { cards } = await srs.deck(deckName(scope));
  const own = cards.filter((c) => c.deck === deckName(scope));
  const md = `#flashcards\n\n${own.map((c) => `${c.front.replace(/\n/g, ' ')}::${c.back.replace(/\n/g, ' ')} ${c.cite || ''}`.trim()).join('\n\n')}\n`;
  const p = studioPath(scope, 'Flashcards');
  await writeNote(p, md);
  return p;
}

export async function addCard(scope, card) {
  if (!card?.front || !card?.back) throw new Error('Card needs a front and a back');
  await srs.addCards(deckName(scope), [{ front: String(card.front), back: String(card.back), cite: String(card.cite || '') }]);
  await mirrorDeck(scope);
  return srs.deck(deckName(scope));
}

// Related notes the topic doesn't link to yet, with a one-line reason from the model.
export async function suggestLinks(notePath, notebook) {
  const { relatedNotes, scopeText } = await import('./retrieve.js');
  const { linksOf } = await import('./links.js');
  const already = new Set(linksOf(notePath).links);
  const cands = (await relatedNotes(notePath, { prefix: notebook, limit: 8 })).filter((r) => !already.has(r.path) && r.score >= 0.45).slice(0, 5);
  if (!cands.length) return [];
  const own = (await scopeText({ topic: notePath }, 3000)).slice(0, 3000);
  const others = [];
  for (const c of cands) others.push(`[${others.length}] ${c.note}: ${(await scopeText({ topic: c.path }, 600)).replace(/\s+/g, ' ').slice(0, 400)}`);
  const r = await generateJSON(`A student is writing the note below. For each candidate note, say whether linking to it would help, and why in one short sentence.
Return JSON {"links":[{"i":0,"useful":true,"reason":"..."}]}

Note:\n${own}\n\nCandidates:\n${others.join('\n')}`, (o) => Array.isArray(o.links)).catch(() => ({ links: cands.map((_, i) => ({ i, useful: true, reason: '' })) }));
  return r.links.filter((l) => l.useful !== false && cands[l.i]).map((l) => ({ ...cands[l.i], reason: String(l.reason || '') }));
}

// Targeted practice on weak concepts: quiz questions drawn only from those [[Note#Heading]] chunks.
export async function drill(scope, concepts, count = 5) {
  const { getTable, esc } = await import('./indexer.js');
  const table = await getTable();
  const parts = [];
  for (const c of concepts.slice(0, 6)) {
    const m = /\[\[([^\]|#]+)(?:#([^\]|]+))?/.exec(c);
    if (!m) continue;
    const where = `note = '${esc(m[1].trim())}'` + (m[2] ? ` AND heading = '${esc(m[2].trim())}'` : '');
    for (const r of await table.query().where(where).select(['note', 'heading', 'text']).limit(3).toArray()) {
      parts.push(`### [[${r.note}${r.heading ? '#' + r.heading : ''}]]\n${r.text}`);
    }
  }
  if (!parts.length) throw new Error('Couldn’t find the notes behind these weak spots');
  return quiz(scope, { count, difficulty: 'medium', text: parts.join('\n\n') });
}

// Inline AI on a text selection from the editor.
export async function assist(action, text, scope) {
  text = String(text || '').slice(0, 4000);
  if (!text.trim()) throw new Error('Select some text first');
  const ctx = (await retrieve(text, { ...scope, mode: 'focused' })).slice(0, 5)
    .map((c) => `--- [[${c.note}${c.heading ? '#' + c.heading : ''}]]\n${c.text}`).join('\n\n');
  if (action === 'flashcard') {
    const c = await generateJSON(`Turn this passage into ONE high-quality flashcard. Return JSON {"front":"question","back":"concise answer"}.\n\nPassage:\n${text}`, (o) => o.front && o.back);
    return { card: { front: String(c.front), back: String(c.back) } };
  }
  if (action === 'quiz') {
    const q = await generateJSON(`Write ONE multiple-choice question testing understanding of this passage. 4 choices, "answer" is the 0-based index.
Return JSON {"question":"...","choices":["a","b","c","d"],"answer":0,"explanation":"..."}\n\nPassage:\n${text}`,
      (o) => o.question && Array.isArray(o.choices) && o.choices.length === 4 && Number.isInteger(o.answer));
    return { question: q };
  }
  const how = {
    explain: 'Explain this passage clearly for a student, in 2-4 short paragraphs. Use the related notes for context and cite them as [[Note#Heading]] when you use them.',
    simplify: 'Rewrite this passage in simpler, plainer language. Keep every fact. Output only the rewritten text.',
  }[action];
  if (!how) throw new Error(`Unknown action: ${action}`);
  return { text: await generate(`${how}\n\nPassage:\n${text}\n\nRelated notes:\n${ctx || '(none)'}`) };
}

export const deck = (scope) => srs.deck(deckName(scope));
export const review = srs.review;

export async function quiz(scope, { count = 5, difficulty = 'medium', text: given } = {}) {
  const text = given || (await textFor(scope));
  const out = await generateJSON(`Write a ${difficulty}-difficulty multiple-choice quiz with ${count} questions testing understanding (not trivia) of these notes.
Each question has exactly 4 choices, one correct. "answer" is the 0-based index of the correct choice. Plausible distractors.
"explanation" says why the answer is right in 1-2 sentences. ${CITE_RULE}
Return JSON: {"questions":[{"question":"...","choices":["a","b","c","d"],"answer":0,"explanation":"...","cite":"[[Note#Heading]]"}]}

Notes:\n${text}`, (o) => Array.isArray(o.questions) && o.questions.length &&
    o.questions.every((q) => q.question && Array.isArray(q.choices) && q.choices.length === 4 && Number.isInteger(q.answer) && q.answer >= 0 && q.answer < 4));
  // Shuffle choices so the model's answer-position bias doesn't leak.
  return {
    questions: out.questions.map((q) => {
      const order = [0, 1, 2, 3].sort(() => Math.random() - 0.5);
      return { ...q, choices: order.map((i) => String(q.choices[i])), answer: order.indexOf(q.answer) };
    }),
  };
}

// Headings flush-left; bullets re-indented relative to the first bullet after each heading
// (models often indent 4+ spaces, which Markdown would turn into a code block).
function normalizeOutline(md) {
  let base = null;
  return md.split('\n').map((line) => {
    if (/^\s*#/.test(line)) { base = null; return line.trim(); }
    const m = line.match(/^(\s*)[-*+]\s+(.*)/);
    if (!m) return line.trim();
    base ??= m[1].length;
    const depth = Math.max(0, Math.round((m[1].length - base) / 2));
    return `${'  '.repeat(depth)}- ${m[2]}`;
  }).join('\n');
}

export async function mindmap(scope) {
  const text = await textFor(scope);
  let md = await generate(`Create a mind map of the key concepts in these notes as a nested Markdown outline.
Start with a single "# " root title. Use "## " for main branches and "- " bullets (nested with 2 spaces) for sub-points.
Keep each node under 8 words. At the end of leaf bullets add the relevant [[Note#Heading]] tag from the notes.
Output only the outline.

Notes:\n${text}`);
  md = md.replace(/^```\w*\n?|```$/gm, '').replace(/^(\s*)[-*]\s+#+\s*/gm, '$1- ').trim();
  md = normalizeOutline(md);
  const p = studioPath(scope, 'Mind Map');
  await writeNote(p, `---\nmindmap-plugin: basic\n---\n\n${md}\n`);
  return { path: p, markdown: md };
}

export async function explainBack(scope, subject, explanation) {
  const sources = await retrieve(`${subject}\n${explanation}`, { ...scope, mode: 'focused' });
  if (!sources.length) throw new Error('No notes found for this topic');
  const ctx = sources.map((c) => `--- [[${c.note}${c.heading ? '#' + c.heading : ''}]]\n${c.text}`).join('\n\n');
  const { goalsContext } = await import('./meta.js');
  const goals = await goalsContext(scope);
  const r = await generateJSON(`${goals ? `The student's goals for this material:\n${goals}\nWeigh gaps that matter for these goals more heavily.\n\n` : ''}You are a supportive tutor using the Feynman technique. The student explained "${subject}" in their own words.
Grade their explanation ONLY against the reference notes. Be specific and encouraging.
Return JSON: {"score": 0-10, "summary": "one-sentence verdict", "gotRight": ["..."], "gaps": ["important things from the notes they left out"], "misconceptions": ["things they said that contradict the notes"], "cites": ["[[Note#Heading]]"]}

Reference notes:\n${ctx}

Student's explanation:\n${explanation}`, (o) => typeof o.score === 'number');
  const list = (x) => [x].flat(3).filter(Boolean).map(String);
  const { logEvent } = await import('./progress.js');
  const concept = list(r.cites).map((c) => (c.startsWith('[[') ? c : `[[${c.replace(/^\[+|\]+$/g, '')}]]`))[0]
    || (sources[0] ? `[[${sources[0].note}${sources[0].heading ? '#' + sources[0].heading : ''}]]` : '');
  await logEvent({ kind: 'explain', notebook: scope.notebook, concept, score: Math.max(0, Math.min(1, Number(r.score) / 10)), correct: Number(r.score) >= 7, label: subject });
  return { ...r, gotRight: list(r.gotRight), gaps: list(r.gaps), misconceptions: list(r.misconceptions), cites: list(r.cites).map((c) => (c.startsWith('[[') ? c : `[[${c.replace(/^\[+|\]+$/g, '')}]]`)), sources };
}
