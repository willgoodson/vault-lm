// SM-2 spaced repetition, persisted per vault in cards.json.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { vault } from './vaults.js';

const file = () => path.join(vault.data, 'cards.json');
const DAY = 86400000;

export const cardId = (c) => crypto.createHash('sha1').update(c.front + '\0' + c.back).digest('hex').slice(0, 12);

export async function load() { return JSON.parse(await fs.readFile(file(), 'utf8').catch(() => '{}')); }
async function save(db) { await fs.mkdir(vault.data, { recursive: true }); await fs.writeFile(file(), JSON.stringify(db, null, 2)); }

export async function addCards(deck, cards) {
  const db = await load();
  for (const c of cards) {
    const id = cardId(c);
    db[id] ??= { ...c, id, deck, ease: 2.5, interval: 0, reps: 0, due: Date.now() };
  }
  await save(db);
  return deckCards(db, deck);
}

// A deck includes cards from anything nested under it (notebook → sections → notes).
const deckCards = (db, deck) => Object.values(db).filter((c) => c.deck === deck || c.deck.startsWith(deck + '/'));

export async function deck(deckName) {
  const cards = deckCards(await load(), deckName);
  return { cards, due: cards.filter((c) => c.due <= Date.now()).length };
}

export async function decks() {
  const out = {};
  for (const c of Object.values(await load())) {
    out[c.deck] ??= { deck: c.deck, total: 0, due: 0 };
    out[c.deck].total++;
    if (c.due <= Date.now()) out[c.deck].due++;
  }
  return Object.values(out);
}

// grade: 0=again 1=hard 2=good 3=easy
export async function review(id, grade) {
  const db = await load();
  const c = db[id];
  if (!c) throw new Error('Unknown card');
  if (grade === 0) { c.reps = 0; c.interval = 0; c.due = Date.now() + 60_000; }
  else {
    const q = [0, 3, 4, 5][grade];
    c.ease = Math.max(1.3, c.ease + 0.1 - (5 - q) * (0.08 + (5 - q) * 0.02));
    c.reps++;
    c.interval = c.reps === 1 ? (grade === 3 ? 4 : 1) : c.reps === 2 ? 6 : Math.round(c.interval * c.ease * (grade === 1 ? 0.6 : grade === 3 ? 1.3 : 1));
    c.due = Date.now() + c.interval * DAY;
  }
  await save(db);
  const { logEvent } = await import('./progress.js');
  const nb = c.deck.split('/').slice(0, 1).join('/');
  await logEvent({ kind: 'card', notebook: notebookOfDeck ? await notebookOfDeck(c.deck) : nb, concept: c.cite || '', correct: grade >= 2, label: c.front });
  return c;
}

// Resolve which notebook a deck path belongs to (set by the server at startup).
let notebookOfDeck = null;
export const setNotebookResolver = (fn) => (notebookOfDeck = fn);

// Decks are keyed by topic/section/notebook path; keep them attached after renames and moves.
// `moves` = [[fromPath, toPath], ...] (files and/or folders).
export async function rekeyCards(moves) {
  const db = await load();
  let changed = false;
  for (const c of Object.values(db)) {
    for (const [from, to] of moves) {
      if (c.deck === from || c.deck.startsWith(from + '/')) { c.deck = to + c.deck.slice(from.length); changed = true; break; }
    }
  }
  if (changed) await save(db);
}
