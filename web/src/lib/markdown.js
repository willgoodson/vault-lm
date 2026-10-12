import { marked } from 'marked';
import { resolveNote } from '../store.js';

const escAttr = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');

// Render Markdown, turning [[Note#Heading|alias]] into citation chips.
// `sources` (from retrieval) lets us resolve paths and mark outside-notebook refs.
export function render(md, sources = []) {
  const html = marked.parse(md || '', { breaks: false });
  return html.replace(/\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]/g, (_, note, heading, alias) => {
    note = note.trim();
    const src = sources.find((s) => s.note === note && (!heading || s.heading === heading)) || sources.find((s) => s.note === note);
    const path = src?.path || resolveNote(note) || '';
    const label = alias || (heading ? `${note} › ${heading}` : note);
    return `<a class="cite${src?.kind === 'linked' ? ' linked' : src?.kind === 'overview' ? ' overview' : src?.reference ? ' ref' : ''}" data-path="${escAttr(path)}" data-note="${escAttr(note)}" data-heading="${escAttr(heading || '')}" title="Click to preview · ⌘-click to open in Obsidian">${escAttr(label)}</a>`;
  });
}
