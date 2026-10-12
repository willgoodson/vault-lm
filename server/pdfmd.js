// Layout-aware PDF → Markdown. Rebuilds lines from positioned text items, then uses font size/weight to
// recover headings, strips running headers/footers and page numbers, fixes symbol-font bullets,
// re-flows hard-wrapped paragraphs and de-hyphenates. Falls back to plain text when layout is missing.
import { extractTextItems } from 'unpdf';

// Includes \u0000 and private-use glyphs: symbol-font bullets often extract as those.
const BULLET = /^[\u0000\u2022\u2023\u25E6\u2043\u2219\u25AA\u25AB\u25CF\u25CB\u25A0\u25A1\u2013\u2014\-\*·\uF0A7\uF0B7\uF076\uF0D8\uF0FC\uF0A8\uF06C\uF0E0]\s*/;
const isBold = (f) => /bold|black|heavy|semibold|demi/i.test(f || '');
const norm = (s) => s.replace(/\d+/g, '#').replace(/\s+/g, ' ').trim().toLowerCase();

function linesOf(items) {
  const rows = [];
  for (const it of items) {
    if (!it.str || !it.str.trim()) continue;
    const tol = Math.max(2, (it.fontSize || 10) * 0.45);
    let row = rows.find((r) => Math.abs(r.y - it.y) <= tol);
    if (!row) rows.push((row = { y: it.y, items: [] }));
    row.items.push(it);
  }
  return rows
    .sort((a, b) => b.y - a.y) // PDF origin is bottom-left: top of page first
    .map((r) => {
      r.items.sort((a, b) => a.x - b.x);
      let text = '';
      let prevEnd = null;
      for (const it of r.items) {
        if (prevEnd != null && it.x - prevEnd > (it.fontSize || 10) * 0.18 && !text.endsWith(' ') && !it.str.startsWith(' ')) text += ' ';
        text += it.str;
        prevEnd = it.x + (it.width || 0);
      }
      const sizes = r.items.map((i) => i.fontSize || 0);
      const chars = r.items.reduce((n, i) => n + i.str.trim().length, 0);
      const boldChars = r.items.filter((i) => isBold(i.fontFamily || i.fontName)).reduce((n, i) => n + i.str.trim().length, 0);
      return { y: r.y, x: r.items[0].x, text: text.replace(/\s+/g, ' ').trim(), size: Math.max(...sizes), bold: chars > 0 && boldChars / chars > 0.6 };
    })
    .filter((l) => l.text);
}

export async function pdfToMarkdown(pdf) {
  const { items, totalPages } = await extractTextItems(pdf);
  const pages = items.map(linesOf);

  // Body font size = the size carrying the most characters.
  const weight = {};
  for (const p of pages) for (const l of p) { const k = Math.round(l.size * 2) / 2; weight[k] = (weight[k] || 0) + l.text.length; }
  const body = +Object.entries(weight).sort((a, b) => b[1] - a[1])[0]?.[0] || 10;

  // Running headers/footers: the same text (digits ignored) near the top/bottom of many pages.
  // Keyed by position (Nth line from the top / bottom) so repeated body text isn't mistaken for a header.
  const freq = {};
  const edgeKey = (p, i) => (i < 3 ? `t${i}|` : i >= p.length - 3 ? `b${p.length - 1 - i}|` : null);
  for (const p of pages) {
    const seen = new Set();
    p.forEach((l, i) => { const e = edgeKey(p, i); if (e) seen.add(e + norm(l.text)); });
    for (const k of seen) freq[k] = (freq[k] || 0) + 1;
  }
  const minRepeat = Math.max(3, Math.ceil(totalPages * 0.4));
  const running = new Set(Object.entries(freq).filter(([, n]) => n >= minRepeat).map(([k]) => k));
  const isRunning = (p, i, l) => { const e = edgeKey(p, i); return e && running.has(e + norm(l.text)); };

  const blocks = []; // { type: 'h'|'p'|'li', level?, text, page }
  for (const [pi, p] of pages.entries()) {
    const kept = p.filter((l, i) => {
      if (isRunning(p, i, l)) return false;
      if (/^(page\s*)?\d{1,4}(\s*(of|\/)\s*\d+)?$/i.test(l.text)) return false; // page numbers
      if (/(\.\s?){6,}\s*\d+$/.test(l.text)) return false;                       // table-of-contents leaders
      return true;
    });
    let prev = null;
    for (const l of kept) {
      const short = l.text.length < 110 && !/[.;,]$/.test(l.text);
      const big = l.size >= body * 1.3;
      const medium = l.size >= body * 1.1;
      const labeled = /^(chapter|part|section|domain|module|unit|appendix|lesson)\s+[\dIVXLC]+\b/i.test(l.text);
      // Numbered items like "Task 3 …" / "Objective 2 …" act as sub-headings even when the PDF doesn't expose bold.
      const sublabel = /^(task|step|objective|topic|lab|exercise|question|activity|outcome)\s+\d+(\.\d+)*\b/i.test(l.text) && l.text.length < 160;
      let level = 0;
      if (short && (big || (labeled && (medium || l.bold)))) level = 2;
      else if (short && (medium || (l.bold && l.text.length < 90 && !BULLET.test(l.text)))) level = 3;
      else if (short && labeled) level = 2;
      else if (sublabel) level = 3;

      const gap = prev ? prev.y - l.y : 0;
      const sameFlow = prev && gap > 0 && gap < l.size * 1.9 && Math.abs(prev.size - l.size) < 0.6;
      const last = blocks.at(-1);

      if (level) {
        // A heading that wraps onto a second line (same size/weight, tight gap) continues the heading.
        if (last?.type === 'h' && last.page === pi && sameFlow && prev?.bold === l.bold && !sublabel) last.text += ' ' + l.text;
        else blocks.push({ type: 'h', level, text: l.text, page: pi });
      } else if (last?.type === 'h' && last.level === 3 && last.page === pi && sameFlow && !BULLET.test(l.text) && /^[a-z(]/.test(l.text)) {
        last.text = joinWrapped(last.text, l.text); // wrapped "Task 1 Demonstrate … and" / "processes."
      } else if (BULLET.test(l.text)) {
        blocks.push({ type: 'li', text: l.text.replace(BULLET, ''), page: pi, x: l.x });
      } else if (last?.type === 'p' && sameFlow && Math.abs(l.x - last.x) < body * 3) {
        last.text = joinWrapped(last.text, l.text);
      } else if (last?.type === 'li' && sameFlow && (l.x > last.x + body * 0.25 || (/^[a-z(]/.test(l.text) && !/[.!?]$/.test(last.text)))) {
        last.text = joinWrapped(last.text, l.text); // wrapped bullet text is indented past the bullet glyph
      } else if (last && last.page !== pi && (last.type === 'p' || last.type === 'li') && !/[.!?:]$/.test(last.text) && /^[a-z(]/.test(l.text)) {
        last.text = joinWrapped(last.text, l.text); // paragraph continues across a page break
      } else {
        blocks.push({ type: 'p', text: l.text, page: pi, x: l.x });
      }
      prev = l;
    }
  }

  // Don't let a first-page title become a duplicate of the note title; keep everything else.
  const md = [];
  for (const b of blocks) {
    if (b.type === 'h') md.push('', `${'#'.repeat(b.level)} ${b.text}`, '');
    else if (b.type === 'li') md.push(`- ${b.text}`);
    else md.push('', b.text, '');
  }
  const text = md.join('\n').replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\uE000-\uF8FF]/g, '').replace(/\n{3,}/g, '\n\n').trim();
  return { text, totalPages, chars: text.replace(/\s/g, '').length, removed: [...running].map((k) => k.split('|')[1]) };
}

function joinWrapped(a, b) {
  if (/[A-Za-z]-$/.test(a) && /^[a-z]/.test(b)) return a.slice(0, -1) + b; // de-hyphenate
  return `${a} ${b}`;
}
