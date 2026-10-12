// Import outside material as Markdown sources: web pages, YouTube transcripts, PDFs (with OCR for scans),
// EPUBs, images (OCR) and plain text. Everything is converted locally.
import path from 'node:path';
import { extractText, getDocumentProxy, renderPageAsImage } from 'unpdf';
import { JSDOM } from 'jsdom';
import { Readability } from '@mozilla/readability';
import TurndownService from 'turndown';
import JSZip from 'jszip';
import { writeNote, saveAttachment } from './files.js';
import { DATA } from './config.js';
import { pdfToMarkdown } from './pdfmd.js';
import { emit } from './events.js';

const safe = (s) => s.replace(/[\\/:*?"<>|#^[\]]/g, '').replace(/\s+/g, ' ').trim().slice(0, 100) || 'Untitled';
const turndown = () => new TurndownService({ headingStyle: 'atx', codeBlockStyle: 'fenced', bulletListMarker: '-' });

async function save(dir, title, meta, body) {
  const rel = path.join(dir, `${safe(title)}.md`);
  const fm = Object.entries({ ...meta, added: new Date().toISOString() }).filter(([, v]) => v != null && v !== '').map(([k, v]) => `${k}: ${JSON.stringify(v)}`).join('\n');
  await writeNote(rel, `---\n${fm}\n---\n\n# ${title}\n\n${body.trim()}\n`);
  return rel;
}

// Long-running imports report progress to the UI over SSE.
const job = (id, label) => ({
  update: (done, total, note) => emit('job', { id, label, done, total, note }),
  end: () => emit('job', { id, label, done: 1, total: 1, finished: true }),
});

// ---------- URLs
const youtubeId = (url) => {
  const m = /(?:youtube\.com\/(?:watch\?(?:.*&)?v=|shorts\/|embed\/|live\/)|youtu\.be\/)([\w-]{11})/.exec(url);
  return m?.[1] || null;
};
const ts = (ms) => { const s = Math.floor(ms / 1000); const h = Math.floor(s / 3600); const mm = String(Math.floor((s % 3600) / 60)).padStart(2, '0'); const ss = String(s % 60).padStart(2, '0'); return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`; };
const decodeEntities = (s) => s.replace(/&amp;#39;|&#39;/g, "'").replace(/&amp;quot;|&quot;/g, '"').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>');

async function addYouTube(dir, url, id) {
  const { YoutubeTranscript } = await import('youtube-transcript');
  let items;
  // Prefer English captions, then whatever the video has.
  try { items = await YoutubeTranscript.fetchTranscript(id, { lang: 'en' }); }
  catch {
    try { items = await YoutubeTranscript.fetchTranscript(id); }
    catch (e) { throw new Error(`No transcript available for this video (${e.message.replace(/^\[YoutubeTranscript\] 🚨 /, '')})`); }
  }
  // Title/channel via oEmbed (no API key).
  const info = await fetch(`https://www.youtube.com/oembed?url=${encodeURIComponent(`https://www.youtube.com/watch?v=${id}`)}&format=json`).then((r) => (r.ok ? r.json() : {})).catch(() => ({}));
  // srv3 captions report integer milliseconds; the classic format reports fractional seconds.
  const inSeconds = items.some((it) => !Number.isInteger(it.offset));
  const msOf = (it) => (inSeconds ? it.offset * 1000 : it.offset);
  // Group captions into ~60-second paragraphs with timestamps.
  const blocks = [];
  let cur = null;
  for (const it of items) {
    const at = msOf(it);
    if (!cur || at - cur.start > 60000) { cur = { start: at, text: [] }; blocks.push(cur); }
    cur.text.push(decodeEntities(it.text).replace(/\s+/g, ' ').trim());
  }
  const body = blocks.map((b) => `## ${ts(b.start)}\n\n${b.text.join(' ')}`).join('\n\n');
  return save(dir, info.title || `YouTube ${id}`, { source: `https://www.youtube.com/watch?v=${id}`, channel: info.author_name, type: 'youtube' }, body);
}

export async function addUrl(dir, url) {
  const yt = youtubeId(url);
  if (yt) return addYouTube(dir, url, yt);
  const res = await fetch(url, { headers: { 'user-agent': 'Mozilla/5.0 vault-lm' } });
  if (!res.ok) throw new Error(`Fetch failed: ${res.status}`);
  const type = res.headers.get('content-type') || '';
  if (type.includes('application/pdf')) return addFile(dir, path.basename(new URL(url).pathname) || 'document.pdf', Buffer.from(await res.arrayBuffer()), url);
  const dom = new JSDOM(await res.text(), { url });
  const article = new Readability(dom.window.document).parse();
  if (!article) throw new Error('Could not extract article content');
  return save(dir, article.title || url, { source: url, type: 'web' }, turndown().turndown(article.content));
}

// ---------- OCR (tesseract.js, WASM; the English model is downloaded once and cached locally)
let workerP;
async function ocr(image, onProgress) {
  const { createWorker } = await import('tesseract.js');
  workerP ??= createWorker('eng', 1, { cachePath: path.join(DATA, 'tesseract'), logger: (m) => m.status === 'recognizing text' && onProgress?.(m.progress) });
  const w = await workerP;
  const { data } = await w.recognize(Buffer.from(image));
  return data.text.replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
}

// ---------- files
export async function addFile(dir, filename, buffer, sourceUrl, opts = {}) {
  const ext = path.extname(filename).toLowerCase();
  const title = path.basename(filename, path.extname(filename));
  if (ext === '.pdf') return addPdf(dir, filename, buffer, sourceUrl, opts);
  if (ext === '.epub') return addEpub(dir, filename, buffer);
  if (['.png', '.jpg', '.jpeg', '.webp', '.gif', '.bmp', '.tif', '.tiff'].includes(ext)) return addImage(dir, filename, buffer);
  if (['.txt', '.md', '.markdown'].includes(ext)) return save(dir, title, { source: filename, type: 'text' }, buffer.toString('utf8').replace(/^---\n[\s\S]*?\n---\n/, ''));
  if (['.html', '.htm'].includes(ext)) {
    const dom = new JSDOM(buffer.toString('utf8'));
    const article = new Readability(dom.window.document).parse();
    return save(dir, article?.title || title, { source: filename, type: 'web' }, turndown().turndown(article?.content || dom.window.document.body.innerHTML));
  }
  throw new Error(`Unsupported file type: ${ext || filename}`);
}

export async function addPdf(dir, filename, buffer, sourceUrl, { original } = {}) {
  const title = path.basename(filename, path.extname(filename));
  const pdf = await getDocumentProxy(new Uint8Array(buffer));
  let body = '', ocrUsed = false, totalPages = pdf.numPages;
  try {
    const r = await pdfToMarkdown(pdf);
    totalPages = r.totalPages;
    if (r.chars / Math.max(1, totalPages) >= 50) body = r.text;
  } catch (e) { console.error('layout extraction failed, falling back:', e.message); }
  if (!body) {
    const { text } = await extractText(pdf, { mergePages: false });
    const chars = text.reduce((n, t) => n + t.trim().length, 0);
    if (chars / Math.max(1, totalPages) >= 50) body = text.map((t, i) => `## Page ${i + 1}\n\n${t.trim()}`).join('\n\n');
  }
  // A scanned PDF has (almost) no text layer — render each page and OCR it.
  if (!body) {
    ocrUsed = true;
    const j = job(`ocr-${Date.now()}`, `OCR: ${filename}`);
    const pages = [];
    for (let i = 1; i <= totalPages; i++) {
      j.update(i - 1, totalPages, `Page ${i} of ${totalPages}`);
      const img = await renderPageAsImage(pdf, i, { canvasImport: () => import('@napi-rs/canvas'), scale: 2 });
      pages.push(await ocr(img));
    }
    j.end();
    body = pages.map((t, i) => `## Page ${i + 1}\n\n${t.trim()}`).join('\n\n');
  }
  // Keep the original next to the source, so it can be opened and re-converted later.
  const att = original ? { path: original, name: path.basename(original) } : await saveAttachment(path.join(dir, `${safe(title)}.md`), `${title}.pdf`, buffer);
  return save(dir, title, { source: sourceUrl || filename, type: 'pdf', pages: totalPages, original: att.path, ocr: ocrUsed || undefined },
    `> Original: [[${att.name}]] · ${totalPages} pages\n\n${body}`);
}

// Re-run conversion for an existing source from its saved original (after converter improvements).
export async function reprocess(rel) {
  const { readNote } = await import('./files.js');
  const { content } = await readNote(rel);
  const original = /^original:\s*"?([^"\n]+)"?/m.exec(content)?.[1];
  if (!original) throw Object.assign(new Error('This source has no saved original to re-convert. Upload the file again.'), { statusCode: 400 });
  const fsMod = await import('node:fs/promises');
  const { safePath, trash } = await import('./files.js');
  const buf = await fsMod.readFile(safePath(original));
  await trash(rel);
  return addFile(path.dirname(rel), path.basename(original), buf, undefined, { original });
}

async function addImage(dir, filename, buffer) {
  const j = job(`ocr-${Date.now()}`, `OCR: ${filename}`);
  j.update(0, 1, 'Reading text…');
  const text = await ocr(buffer, (p) => j.update(p, 1, 'Reading text…'));
  j.end();
  // Keep the original image next to the source note and embed it.
  const sourceRel = path.join(dir, `${safe(path.basename(filename, path.extname(filename)))}.md`);
  const att = await saveAttachment(sourceRel, filename, buffer);
  return save(dir, path.basename(filename, path.extname(filename)), { source: filename, type: 'image', ocr: true }, `![[${att.name}]]\n\n${text || '_No text found in this image._'}`);
}

async function addEpub(dir, filename, buffer) {
  const zip = await JSZip.loadAsync(buffer);
  const container = await zip.file('META-INF/container.xml')?.async('string');
  const opfPath = container && /full-path="([^"]+)"/.exec(container)?.[1];
  if (!opfPath) throw new Error('Not a valid EPUB (no package file)');
  const opf = new JSDOM(await zip.file(opfPath).async('string'), { contentType: 'text/xml' }).window.document;
  const base = path.posix.dirname(opfPath);
  const manifest = Object.fromEntries([...opf.querySelectorAll('manifest > item')].map((i) => [i.getAttribute('id'), i.getAttribute('href')]));
  const spine = [...opf.querySelectorAll('spine > itemref')].map((r) => manifest[r.getAttribute('idref')]).filter(Boolean);
  const meta = (tag) => opf.getElementsByTagName(tag)[0]?.textContent?.trim();
  const title = meta('dc:title') || path.basename(filename, '.epub');
  const td = turndown();
  const chapters = [];
  const j = job(`epub-${Date.now()}`, `EPUB: ${title}`);
  for (const [i, href] of spine.entries()) {
    j.update(i, spine.length, `Chapter ${i + 1} of ${spine.length}`);
    const f = zip.file(path.posix.join(base, decodeURIComponent(href)));
    if (!f) continue;
    const doc = new JSDOM(await f.async('string')).window.document;
    doc.querySelectorAll('img, svg, script, style').forEach((n) => n.remove());
    const heading = doc.querySelector('h1, h2, h3, title')?.textContent?.trim();
    const md = td.turndown(doc.body?.innerHTML || '').replace(/^#{1,6}\s+.*\n+/, '').trim();
    if (md.length < 40) continue;
    chapters.push(`## ${heading || `Chapter ${chapters.length + 1}`}\n\n${md.replace(/^(#{1,4})\s/gm, '#$1 ')}`);
  }
  j.end();
  if (!chapters.length) throw new Error('No readable chapters found in this EPUB');
  return save(dir, title, { source: filename, author: meta('dc:creator'), type: 'epub' }, chapters.join('\n\n'));
}
