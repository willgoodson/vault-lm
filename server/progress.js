// Study progress: an append-only event log (data/vaults/<slug>/progress.jsonl) of quiz answers,
// card grades and explain-back scores, plus derived weak spots and dashboard stats.
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';
import * as srs from './srs.js';
import { listNotebooks } from './layout.js';
import { loadMeta } from './meta.js';

const file = () => path.join(vault.data, 'progress.jsonl');
const DAY = 86400000;
const HALF_LIFE = 14 * DAY;

/** e: { kind: 'quiz'|'card'|'explain', notebook, concept: '[[Note#Heading]]', correct: bool, score?: 0..1, label? } */
export async function logEvent(e) {
  if (!e?.kind) return;
  await fs.mkdir(vault.data, { recursive: true });
  await fs.appendFile(file(), JSON.stringify({ t: Date.now(), ...e }) + '\n');
}

export async function events() {
  const raw = await fs.readFile(file(), 'utf8').catch(() => '');
  return raw.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
}

const parseCite = (c) => {
  const m = /\[\[([^\]|#]+)(?:#([^\]|]+))?/.exec(c || '');
  return m ? { note: m[1].trim(), heading: (m[2] || '').trim() } : null;
};

// Concepts you keep missing: recency-weighted accuracy per cited [[Note#Heading]].
export async function weakSpots({ notebook, limit = 8 } = {}) {
  const now = Date.now();
  const by = new Map();
  for (const e of await events()) {
    if (notebook && e.notebook !== notebook) continue;
    if (!e.concept || !parseCite(e.concept)) continue;
    const w = Math.pow(0.5, (now - e.t) / HALF_LIFE);
    const right = e.score != null ? e.score : e.correct ? 1 : 0;
    const s = by.get(e.concept) || { concept: e.concept, ...parseCite(e.concept), w: 0, right: 0, attempts: 0, misses: 0, last: 0, labels: new Set(), notebook: e.notebook };
    s.w += w; s.right += w * right; s.attempts++; if (right < 0.5) s.misses++; s.last = Math.max(s.last, e.t);
    if (e.label && right < 0.5) s.labels.add(e.label);
    by.set(e.concept, s);
  }
  return [...by.values()]
    .map((s) => ({ ...s, labels: [...s.labels].slice(0, 3), accuracy: s.w ? s.right / s.w : 0 }))
    .filter((s) => s.misses > 0 && s.accuracy < 0.8)
    .sort((a, b) => a.accuracy - b.accuracy || b.last - a.last)
    .slice(0, limit);
}

const dayKey = (t) => new Date(t).toISOString().slice(0, 10);

export async function summary({ notebook } = {}) {
  const nbs = await listNotebooks();
  const meta = await loadMeta();
  const nbOf = (p) => nbs.find((n) => p === n.path || p.startsWith(n.path + '/'))?.path;
  const all = await events();
  const evs = notebook ? all.filter((e) => e.notebook === notebook) : all;

  // Cards due, per notebook and deck.
  const db = await srs.load();
  const cards = Object.values(db).filter((c) => !notebook || nbOf(c.deck) === notebook);
  const now = Date.now();
  const decks = {};
  for (const c of cards) {
    const nb = nbOf(c.deck) || '(other)';
    decks[nb] ??= { notebook: nb, name: nb.split('/').pop(), total: 0, due: 0, learned: 0 };
    decks[nb].total++;
    if (c.due <= now) decks[nb].due++;
    if (c.reps > 0) decks[nb].learned++;
  }

  // Activity heatmap (12 weeks) and quiz accuracy by day.
  const days = {};
  const acc = {};
  for (const e of evs) {
    const k = dayKey(e.t);
    days[k] = (days[k] || 0) + 1;
    if (e.kind === 'quiz') { acc[k] ??= { right: 0, n: 0 }; acc[k].n++; if (e.correct) acc[k].right++; }
  }
  const heat = [];
  for (let i = 83; i >= 0; i--) { const k = dayKey(now - i * DAY); heat.push({ date: k, count: days[k] || 0 }); }
  let streak = 0;
  for (let i = heat.length - 1; i >= 0 && heat[i].count > 0; i--) streak++;
  const accuracy = Object.entries(acc).sort(([a], [b]) => a.localeCompare(b)).slice(-30).map(([date, v]) => ({ date, pct: v.right / v.n }));

  // Notebooks/sections with due dates, and cards due before then.
  const upcoming = Object.entries(meta)
    .filter(([p, m]) => m.due && (!notebook || p === notebook || p.startsWith(notebook + '/')) && m.status !== 'done')
    .map(([p, m]) => {
      const dueAt = new Date(m.due + 'T23:59').getTime();
      const inScope = Object.values(db).filter((c) => c.deck === p || c.deck.startsWith(p + '/'));
      return { path: p, name: p.split('/').pop(), due: m.due, daysLeft: Math.ceil((dueAt - now) / DAY), cardsDueBefore: inScope.filter((c) => c.due <= dueAt).length, total: inScope.length };
    })
    .sort((a, b) => a.due.localeCompare(b.due));

  return {
    decks: Object.values(decks).sort((a, b) => b.due - a.due),
    dueTotal: cards.filter((c) => c.due <= now).length,
    learned: cards.filter((c) => c.reps > 0).length,
    totalCards: cards.length,
    heat, streak, accuracy,
    quizAnswered: evs.filter((e) => e.kind === 'quiz').length,
    weak: await weakSpots({ notebook }),
    upcoming,
  };
}

// Due cards across decks (optionally one notebook), for a mixed review session.
export async function dueCards({ notebook, limit = 50 } = {}) {
  const db = await srs.load();
  const now = Date.now();
  return Object.values(db)
    .filter((c) => c.due <= now && (!notebook || c.deck === notebook || c.deck.startsWith(notebook + '/')))
    .sort((a, b) => a.due - b.due)
    .slice(0, limit);
}
