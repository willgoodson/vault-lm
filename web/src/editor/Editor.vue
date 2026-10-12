<script setup>
import { ref, onMounted, onBeforeUnmount, watch } from 'vue';
import { EditorView, keymap, drawSelection, placeholder, dropCursor } from '@codemirror/view';
import { EditorState, Prec } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { syntaxHighlighting, HighlightStyle, indentOnInput } from '@codemirror/language';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete';
import { tags as t } from '@lezer/highlight';
import { AlertTriangle } from 'lucide-vue-next';
import { livePreview, frontmatter, focusField, focusTracker } from './livePreview.js';
import { completions } from './autocomplete.js';
import { store, openTab, resolveNote, fileUrl, onFileEvent, toast, refreshFiles } from '../store.js';
import { api } from '../api.js';

const props = defineProps({ path: String, content: String, mtime: Number });
const emit = defineEmits(['saved', 'selection', 'status']);
const host = ref(null);
const status = ref('saved'); // 'saved' | 'dirty' | 'saving' | 'conflict'
const conflict = ref(null);
let view, baseMtime = props.mtime, saveTimer, tags = [];
watch(status, (s) => emit('status', s), { immediate: true });

api('/api/tags').then((r) => (tags = r)).catch(() => {});

// Resolve a link target to a vault path, Obsidian-style (by name or path).
function resolve(target) {
  if (!target) return props.path;
  const byNote = resolveNote(target);
  if (byNote) return byNote;
  const hit = (function find(nodes) {
    for (const n of nodes) {
      if (n.type === 'file' && (n.path === target || n.name === target.split('/').pop())) return n.path;
      if (n.type === 'dir') { const r = find(n.children); if (r) return r; }
    }
  })(store.tree);
  return hit || null;
}

const highlight = HighlightStyle.define([
  { tag: t.heading, fontWeight: '650' },
  { tag: t.strong, fontWeight: '650' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.link, class: 'cm-tok-link' },
  { tag: t.url, class: 'cm-tok-url' },
  { tag: t.monospace, class: 'cm-tok-mono' },
  { tag: [t.processingInstruction, t.meta], class: 'cm-tok-meta' },
  { tag: t.keyword, class: 'cm-tok-kw' }, { tag: t.string, class: 'cm-tok-str' }, { tag: t.comment, class: 'cm-tok-comment' },
  { tag: [t.number, t.bool], class: 'cm-tok-num' }, { tag: [t.function(t.variableName), t.typeName], class: 'cm-tok-fn' },
]);

async function save(force = false) {
  clearTimeout(saveTimer);
  if (!view) return;
  const content = view.state.doc.toString();
  status.value = 'saving';
  try {
    const r = await api('/api/note', { path: props.path, content, baseMtime: force ? null : baseMtime }, 'PUT');
    baseMtime = r.mtime;
    status.value = view.state.doc.toString() === content ? 'saved' : 'dirty';
    conflict.value = null;
    emit('saved', { mtime: r.mtime, content });
  } catch (e) {
    const body = await fetch(`/api/note?path=${encodeURIComponent(props.path)}`).then((r) => r.json()).catch(() => null);
    if (body && body.mtime !== baseMtime) { status.value = 'conflict'; conflict.value = body; }
    else { status.value = 'dirty'; toast(`Couldn't save: ${e.message}`, 'error'); }
  }
}
const schedule = () => { status.value = 'dirty'; clearTimeout(saveTimer); saveTimer = setTimeout(save, 800); };

function replaceDoc(text) {
  const sel = Math.min(view.state.selection.main.head, text.length);
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, selection: { anchor: sel }, annotations: [] });
}
function reloadTheirs() { replaceDoc(conflict.value.content); baseMtime = conflict.value.mtime; status.value = 'saved'; conflict.value = null; }
function keepMine() { save(true); }

// Edits from outside (Obsidian, iCloud, another tab): reload silently if we have no unsaved changes.
const off = onFileEvent(async (e) => {
  if (e.path !== props.path || !e.mtime || Math.abs(e.mtime - baseMtime) <= 1 || status.value === 'saving') return;
  const fresh = await api(`/api/note?path=${encodeURIComponent(props.path)}`).catch(() => null);
  if (!fresh || fresh.content === view.state.doc.toString()) { if (fresh) baseMtime = fresh.mtime; return; }
  if (status.value === 'saved') { replaceDoc(fresh.content); baseMtime = fresh.mtime; }
  else { status.value = 'conflict'; conflict.value = fresh; }
});

async function uploadImages(files, pos) {
  for (const f of files) {
    if (!f.type.startsWith('image/')) continue;
    const fd = new FormData();
    fd.append('file', f, f.name || 'Pasted image.png');
    try {
      const r = await api(`/api/attachment?note=${encodeURIComponent(props.path)}`, fd);
      const text = `![[${r.name}]]`;
      view.dispatch({ changes: { from: pos, insert: text }, selection: { anchor: pos + text.length } });
      pos += text.length;
      refreshFiles();
    } catch (e) { toast(e.message, 'error'); }
  }
}

async function openLink(target, heading) {
  const p = resolve(target);
  if (p) return openTab(p, heading || undefined);
  // Unresolved link: create the topic next to this note (Obsidian does the same).
  if (!confirm(`“${target}” doesn't exist yet. Create it?`)) return;
  const folder = props.path.split('/').slice(0, -1).join('/');
  const r = await api('/api/topic', { folder, name: target.split('/').pop() });
  await refreshFiles();
  openTab(r.path);
}

onMounted(() => {
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.content,
      extensions: [
        history(), drawSelection(), dropCursor(), indentOnInput(), closeBrackets(), highlightSelectionMatches(),
        EditorView.lineWrapping,
        EditorView.theme({
          '.cm-tooltip.cm-tooltip-autocomplete': { border: 'none', background: 'transparent' },
          '.cm-tooltip.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--font-sans)', maxHeight: '16rem', minWidth: '16rem' },
          '.cm-tooltip.cm-tooltip-autocomplete > ul > li': { padding: '4px 8px', borderRadius: '8px', lineHeight: '1.4' },
          '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': { background: 'color-mix(in oklch, var(--color-accent-500) 15%, transparent)', color: 'inherit' },
          '.cm-completionMatchedText': { textDecoration: 'none', fontWeight: '600', color: 'var(--color-accent-600)' },
        }),
        placeholder('Start writing… type [[ to link a note, # for a tag'),
        markdown({ base: markdownLanguage, codeLanguages: languages }),
        syntaxHighlighting(highlight),
        keymap.of([...closeBracketsKeymap, ...completionKeymap, ...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab,
          { key: 'Mod-s', run: () => (save(), true) },
          { key: 'Mod-b', run: (v) => wrap(v, '**') }, { key: 'Mod-i', run: (v) => wrap(v, '*') }]),
        // Obsidian-style: Enter on an empty list item / quote line ends it instead of continuing.
        Prec.high(keymap.of([{ key: 'Enter', run: (v) => {
          const r = v.state.selection.main;
          if (!r.empty) return false;
          const line = v.state.doc.lineAt(r.head);
          if (r.head !== line.to) return false;
          const m = /^(\s*)(?:[-*+]|\d+[.)])(?:\s+\[[ xX]\])?\s*$/.exec(line.text) || /^(\s*)>\s*$/.exec(line.text);
          if (!m) return false;
          const indent = m[1].length >= 2 ? m[1].slice(2) : '';
          v.dispatch({ changes: { from: line.from, to: line.to, insert: indent } });
          return true;
        } }])),
        focusField, focusTracker, frontmatter,
        livePreview({ resolve, fileUrl }),
        completions({ notes: () => store.palette, tags: () => tags }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) schedule();
          if (u.selectionSet || u.focusChanged) {
            const r = u.state.selection.main;
            const coords = !r.empty && u.view.hasFocus ? u.view.coordsAtPos(r.from) : null;
            emit('selection', coords ? { text: u.state.sliceDoc(r.from, r.to), top: coords.top, left: coords.left, from: r.from, to: r.to } : null);
          }
        }),
        EditorView.domEventHandlers({
          mousedown(e) {
            const wl = e.target.closest('.cm-wikilink');
            if (wl) { e.preventDefault(); openLink(wl.dataset.target, wl.dataset.heading); return true; }
            const ml = e.target.closest('.cm-md-link');
            if (ml && (e.metaKey || e.ctrlKey || !ml.closest('.cm-activeLine'))) {
              const href = decodeURIComponent(ml.dataset.href || '');
              if (/^https?:/.test(href)) { window.open(href, '_blank'); e.preventDefault(); return true; }
              if (href) { e.preventDefault(); openLink(href.replace(/\.md$/, '').split('#')[0], href.split('#')[1]); return true; }
            }
            if (e.target.closest('.cm-properties')) {
              e.preventDefault();
              view.focus();
              view.dispatch({ selection: { anchor: 4 } });
              return true;
            }
          },
          paste(e) { const files = [...(e.clipboardData?.files || [])]; if (files.length) { e.preventDefault(); uploadImages(files, view.state.selection.main.head); return true; } },
          drop(e) {
            const files = [...(e.dataTransfer?.files || [])].filter((f) => f.type.startsWith('image/'));
            if (!files.length) return false;
            e.preventDefault();
            uploadImages(files, view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? view.state.doc.length);
            return true;
          },
        }),
      ],
    }),
  });
});

function wrap(v, m) {
  const r = v.state.selection.main;
  v.dispatch({ changes: [{ from: r.from, insert: m }, { from: r.to, insert: m }], selection: { anchor: r.from + m.length, head: r.to + m.length } });
  return true;
}

onBeforeUnmount(() => { if (status.value === 'dirty') save(); off(); view?.destroy(); });
watch(() => props.path, () => {}); // a new path always means a new component (keyed)

function scrollToHeading(h) {
  const doc = view.state.doc;
  for (let i = 1; i <= doc.lines; i++) {
    const l = doc.line(i);
    if (/^#{1,6}\s/.test(l.text) && l.text.replace(/^#{1,6}\s+/, '').trim() === h) {
      view.dispatch({ selection: { anchor: l.to }, effects: EditorView.scrollIntoView(l.from, { y: 'start', yMargin: 80 }) });
      view.focus();
      return;
    }
  }
}
function replaceRange(from, to, text) { view.dispatch({ changes: { from, to, insert: text } }); }
function insertAtCursor(text) { const p = view.state.selection.main.head; view.dispatch({ changes: { from: p, insert: text }, selection: { anchor: p + text.length } }); view.focus(); }
defineExpose({ scrollToHeading, replaceRange, insertAtCursor, save, getText: () => view.state.doc.toString() });
</script>

<template>
  <div class="relative">
    <div v-if="conflict" class="sticky top-0 z-10 mb-3 flex items-center gap-2 rounded-xl border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-500/40 dark:bg-amber-500/10 dark:text-amber-200">
      <AlertTriangle class="size-4 shrink-0" />
      <span class="flex-1">This note changed outside Vault-LM (maybe in Obsidian or iCloud).</span>
      <button class="btn-outline !py-1 text-xs" @click="reloadTheirs">Load theirs</button>
      <button class="btn-primary !py-1 text-xs" @click="keepMine">Keep mine</button>
    </div>
    <div ref="host" class="vlm-editor" />
  </div>
</template>
