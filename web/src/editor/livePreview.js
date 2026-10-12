// Obsidian-style live preview: Markdown syntax is hidden and rendered on every line except
// the one(s) holding the cursor, where the raw Markdown shows for editing.
import { ViewPlugin, Decoration, EditorView } from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { WikiLinkWidget, ImageWidget, CheckboxWidget, BulletWidget, HrWidget, PropertiesWidget, CalloutWidget } from './widgets.js';

const hide = Decoration.replace({});
const mark = (cls) => Decoration.mark({ class: cls });
const line = (cls) => Decoration.line({ class: cls });
const CODE = new Set(['InlineCode', 'FencedCode', 'CodeBlock', 'CodeText']);

function activeLines(state) {
  const set = new Set();
  for (const r of state.selection.ranges) {
    const a = state.doc.lineAt(r.from).number, b = state.doc.lineAt(r.to).number;
    for (let i = a; i <= b; i++) set.add(i);
  }
  return set;
}

export function livePreview({ resolve, fileUrl }) {
  const build = (view) => {
    const { state } = view;
    const active = view.hasFocus ? activeLines(state) : new Set();
    const isActive = (pos) => active.has(state.doc.lineAt(pos).number);
    const decos = [];
    const tree = syntaxTree(state);

    for (const { from, to } of view.visibleRanges) {
      tree.iterate({
        from, to,
        enter: (n) => {
          const name = n.name;
          const act = isActive(n.from);
          const hdr = name.match(/^ATXHeading(\d)$/);
          if (hdr) { decos.push(line(`cm-h cm-h${hdr[1]}`).range(state.doc.lineAt(n.from).from)); return; }
          if (name === 'HeaderMark' && !act) {
            const next = state.sliceDoc(n.to, n.to + 1) === ' ' ? n.to + 1 : n.to;
            if (state.doc.lineAt(n.from).from === n.from) decos.push(hide.range(n.from, next));
            return;
          }
          if (name === 'StrongEmphasis') decos.push(mark('cm-strong').range(n.from, n.to));
          else if (name === 'Emphasis') decos.push(mark('cm-em').range(n.from, n.to));
          else if (name === 'Strikethrough') decos.push(mark('cm-strike').range(n.from, n.to));
          else if (name === 'InlineCode') decos.push(mark('cm-inline-code').range(n.from, n.to));
          else if ((name === 'EmphasisMark' || name === 'StrikethroughMark' || (name === 'CodeMark' && n.node.parent?.name === 'InlineCode')) && !act) decos.push(hide.range(n.from, n.to));
          else if (name === 'Blockquote') {
            for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) {
              const ln = state.doc.line(l);
              const callout = /^>\s*\[!(\w+)\]/.exec(ln.text);
              decos.push(line(callout ? `cm-quote cm-callout-head cm-callout-${callout[1].toLowerCase()}` : 'cm-quote').range(ln.from));
              if (callout && !active.has(l)) {
                const at = ln.from + ln.text.indexOf('[!');
                const end = at + callout[0].length - callout[0].indexOf('[!');
                const hasTitle = ln.text.slice(end - ln.from).trim().length > 0;
                decos.push(Decoration.replace({ widget: new CalloutWidget(callout[1].toLowerCase(), hasTitle) }).range(at, Math.min(ln.to, end + (ln.text[end - ln.from] === ' ' ? 1 : 0))));
              }
            }
          } else if (name === 'QuoteMark' && !act) {
            decos.push(hide.range(n.from, state.sliceDoc(n.to, n.to + 1) === ' ' ? n.to + 1 : n.to));
          } else if (name === 'FencedCode') {
            for (let l = state.doc.lineAt(n.from).number; l <= state.doc.lineAt(n.to).number; l++) decos.push(line('cm-codeblock').range(state.doc.line(l).from));
            return false;
          } else if (name === 'HorizontalRule' && !act) {
            decos.push(Decoration.replace({ widget: new HrWidget() }).range(n.from, n.to));
          } else if (name === 'ListMark' && !act && /^[-*+]$/.test(state.sliceDoc(n.from, n.to))) {
            const isTask = n.node.parent?.getChild('Task');
            if (!isTask) decos.push(Decoration.replace({ widget: new BulletWidget() }).range(n.from, n.to));
            else decos.push(hide.range(n.from, n.to + 1));
          } else if (name === 'TaskMarker') {
            const checked = /x/i.test(state.sliceDoc(n.from, n.to));
            if (!act) decos.push(Decoration.replace({ widget: new CheckboxWidget(checked, n.from) }).range(n.from, n.to));
            if (checked) decos.push(mark('cm-task-done').range(n.to, state.doc.lineAt(n.to).to));
          } else if (name === 'Image' && !act) {
            const url = n.node.getChild('URL');
            if (url) {
              const src = state.sliceDoc(url.from, url.to);
              decos.push(Decoration.replace({ widget: new ImageWidget(/^https?:/.test(src) ? src : fileUrl(resolve(decodeURIComponent(src)) || src)) }).range(n.from, n.to));
            }
            return false;
          } else if (name === 'Link') {
            const url = n.node.getChild('URL');
            const marks = n.node.getChildren('LinkMark');
            if (url && marks.length >= 2) {
              decos.push(Decoration.mark({ class: 'cm-md-link', attributes: { 'data-href': state.sliceDoc(url.from, url.to) } }).range(marks[0].to, marks[1].from));
              if (!act) { decos.push(hide.range(n.from, marks[0].to)); decos.push(hide.range(marks[1].from, n.to)); }
            }
            return false;
          }
        },
      });

      // Wikilinks, embeds and #tags aren't part of CommonMark — scan visible text.
      const text = state.sliceDoc(from, to);
      for (const m of text.matchAll(/(!?)\[\[([^\]|#^]*)(?:#([^\]|^]*))?(?:\^[^\]|]*)?(?:\|([^\]]*))?\]\]/g)) {
        const start = from + m.index, end = start + m[0].length;
        if (CODE.has(tree.resolveInner(start, 1).name)) continue;
        const target = m[2].trim();
        const resolved = resolve(target);
        if (isActive(start)) { decos.push(mark('cm-wikilink-raw').range(start, end)); continue; }
        if (m[1] && /\.(png|jpe?g|gif|webp|svg)$/i.test(target)) {
          decos.push(Decoration.replace({ widget: new ImageWidget(fileUrl(resolved || target), target) }).range(start, end));
        } else {
          decos.push(Decoration.replace({ widget: new WikiLinkWidget(target, m[3] || '', m[4] || '', !!resolved || !target) }).range(start, end));
        }
      }
      for (const m of text.matchAll(/(^|[\s(])#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/gmu)) {
        const start = from + m.index + m[1].length;
        const node = tree.resolveInner(start, 1);
        if (CODE.has(node.name) || /^ATXHeading|HeaderMark/.test(node.name) || /^#{1,6}\s/.test(state.doc.lineAt(start).text) && state.doc.lineAt(start).from === start) continue;
        decos.push(mark('cm-tag').range(start, start + m[2].length + 1));
      }
    }
    return Decoration.set(decos, true);
  };

  return ViewPlugin.fromClass(class {
    constructor(view) { this.decorations = build(view); }
    update(u) { if (u.docChanged || u.viewportChanged || u.selectionSet || u.focusChanged) this.decorations = build(u.view); }
  }, { decorations: (v) => v.decorations });
}

// Frontmatter collapses into a Properties panel unless the cursor is inside it (block decoration → StateField).
export const frontmatter = StateField.define({
  create: (state) => fmDecos(state, false),
  update(value, tr) {
    if (!tr.docChanged && !tr.selection && !tr.effects.length) return value;
    return fmDecos(tr.state, tr.state.field(focusField, false));
  },
  provide: (f) => EditorView.decorations.from(f),
});
export const setFocus = StateEffect.define();
export const focusField = StateField.define({
  create: () => false,
  update: (v, tr) => { for (const e of tr.effects) if (e.is(setFocus)) return e.value; return v; },
});
function fmDecos(state, focused) {
  const m = /^---\n([\s\S]*?)\n---(\n|$)/.exec(state.doc.toString());
  if (!m) return Decoration.none;
  const end = m[0].length - (m[2] ? 1 : 0);
  const cur = state.selection.main.head;
  if (focused && cur <= end) return Decoration.none;
  return Decoration.set([Decoration.replace({ widget: new PropertiesWidget(m[1]), block: true }).range(0, end)]);
}
export const focusTracker = EditorView.focusChangeEffect.of((_, focusing) => setFocus.of(focusing));
