// [[ note / heading completion and #tag completion, from the app's note index.
import { autocompletion } from '@codemirror/autocomplete';

export function completions({ notes, tags }) {
  const wiki = (ctx) => {
    const m = ctx.matchBefore(/\[\[[^\]\n]*$/);
    if (!m) return null;
    const inner = m.text.slice(2);
    // Insert the name and put the cursor after the closing ]] (closeBrackets may already have added it).
    const applyLink = (text) => (view, _c, from, to) => {
      const hasClose = view.state.sliceDoc(to, to + 2) === ']]';
      view.dispatch({ changes: { from, to, insert: hasClose ? text : text + ']]' }, selection: { anchor: from + text.length + 2 } });
    };
    const hash = inner.indexOf('#');
    if (hash >= 0) {
      const note = inner.slice(0, hash).trim();
      const heads = notes().filter((n) => n.heading && n.note.toLowerCase() === note.toLowerCase());
      return { from: m.from + 2 + hash + 1, options: heads.map((h) => ({ label: h.heading, type: 'text', apply: applyLink(h.heading) })), validFor: /^[^\]#|]*$/ };
    }
    const seen = new Set();
    const options = notes().filter((n) => !n.heading && !n.path.includes('/_studio/') && !seen.has(n.note) && seen.add(n.note)).map((n) => ({
      label: n.note, detail: n.path.split('/').slice(0, -1).join(' / '), type: 'class', apply: applyLink(n.note),
    }));
    return { from: m.from + 2, options, validFor: /^[^\]#|]*$/ };
  };
  const tag = (ctx) => {
    const m = ctx.matchBefore(/(?:^|\s)#[\p{L}\p{N}_/-]*$/u);
    if (!m || (m.text.trim() === '#' && !ctx.explicit && /^#\s?$/.test(ctx.state.doc.lineAt(ctx.pos).text.trim()))) return null;
    const from = m.from + m.text.indexOf('#') + 1;
    return { from, options: tags().map((t) => ({ label: t.tag, detail: `${t.count}`, type: 'keyword' })), validFor: /^[\p{L}\p{N}_/-]*$/u };
  };
  return autocompletion({ override: [wiki, tag], icons: false, activateOnTyping: true });
}
