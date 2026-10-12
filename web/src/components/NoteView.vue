<script setup>
import { ref, computed, onMounted, onActivated, nextTick, watch, defineAsyncComponent } from 'vue';
import { ExternalLink, Crosshair, Eye, PencilLine, Check, Loader2, RefreshCcw, Trash2 } from 'lucide-vue-next';
import { store, ns, openTab, openInObsidian, selectNotebook, select, allTopics, fileUrl, onFileEvent, toast, refreshFiles, closeTab } from '../store.js';
import { api } from '../api.js';
import { render } from '../lib/markdown.js';
const Editor = defineAsyncComponent(() => import('../editor/Editor.vue'));
const NoteSidePanel = defineAsyncComponent(() => import('./NoteSidePanel.vue'));
import SelectionToolbar from './SelectionToolbar.vue';

const props = defineProps({ path: String });
const note = ref(null);
const error = ref('');
const body = ref(null);
const scroller = ref(null);
const mode = ref(localStorage.vlmNoteMode || 'edit'); // 'edit' | 'read'
const name = computed(() => props.path.split('/').pop().replace(/\.md$/, ''));
const crumbs = computed(() => props.path.split('/').slice(0, -1));

async function load() {
  try { note.value = await api(`/api/note?path=${encodeURIComponent(props.path)}`); error.value = ''; }
  catch (e) { error.value = e.message; }
}
onMounted(async () => { await load(); restoreScroll(); jump(); });
onActivated(() => { load(); restoreScroll(); jump(); });

// Reading view
const html = computed(() => {
  if (!note.value) return '';
  const md = note.value.content.replace(/^---\n[\s\S]*?\n---\n/, '')
    .replace(/!\[\[([^\]|]+\.(png|jpe?g|gif|webp|svg))(\|[^\]]*)?\]\]/gi, (_, f) => `![](${fileUrl(resolveEmbed(f))})`)
    .replace(/!\[\[[^\]]+\.wav\]\]\n?/g, '');
  return render(md);
});
const resolveEmbed = (f) => store.tree && (findFile(store.tree, f) || f);
function findFile(nodes, name) {
  for (const n of nodes) {
    if (n.type === 'file' && (n.path === name || n.name === name.split('/').pop())) return n.path;
    if (n.type === 'dir') { const r = findFile(n.children, name); if (r) return r; }
  }
}
const isAudio = computed(() => /!\[\[[^\]]+\.wav\]\]/.test(note.value?.content || ''));
const audioSrc = computed(() => fileUrl(props.path.replace(/\.md$/, '.wav')));

function scrollToHeading(h) {
  if (mode.value === 'edit') return editor.value?.scrollToHeading(h);
  const el = [...(body.value?.querySelectorAll('h1,h2,h3,h4,h5,h6') ?? [])].find((x) => x.textContent.trim() === h);
  el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
}
function jump() {
  const h = ns.value?.heading;
  if (h && ns.value.active === props.path) { ns.value.heading = null; nextTick(() => setTimeout(() => scrollToHeading(h), 150)); }
}
watch(() => ns.value?.heading, jump);

// Remember scroll per tab (per notebook).
function onScroll(e) { if (ns.value) ns.value.scroll[props.path] = e.target.scrollTop; }
function restoreScroll() { nextTick(() => { if (scroller.value) scroller.value.scrollTop = ns.value?.scroll[props.path] || 0; }); }

// External changes (Obsidian, iCloud…) — reload the reading view; the editor handles its own.
onFileEvent((e) => { if (e.path === props.path && mode.value === 'read') load(); });

const editor = ref(null);
const saveStatus = ref('saved');
const reconverting = ref(false);
const isSource = computed(() => props.path.split('/').includes('sources'));
async function removeSource() {
  if (!confirm(`Remove the source “${name.value}”? It moves to the vault trash (.trash/), along with its original file.`)) return;
  try { await api('/api/source/remove', { path: props.path }); closeTab(props.path); await refreshFiles(); toast(`Removed “${name.value}”`, 'success'); }
  catch (e) { toast(e.message, 'error'); }
}
async function reconvert() {
  if (!confirm('Re-convert this source from its original file? The current version moves to .trash, including any edits you made to it.')) return;
  reconverting.value = true;
  try { await api('/api/source/reprocess', { path: props.path }); await load(); toast('Source re-converted', 'success'); }
  catch (e) { toast(e.message, 'error'); }
  reconverting.value = false;
}
const sel = ref(null);
let selTimer;
// Show the AI toolbar shortly after a selection settles (not while dragging).
function onSelection(s) { clearTimeout(selTimer); if (!s || s.text.trim().length < 4) { sel.value = null; return; } selTimer = setTimeout(() => (sel.value = s), 350); }
function replaceSel(text) { if (sel.value) editor.value?.replaceRange(sel.value.from, sel.value.to, text); }
function insertText(t) {
  if (mode.value !== 'edit') setMode('edit');
  nextTick(() => setTimeout(() => editor.value?.insertAtCursor(t), 50));
}
function setMode(m) { mode.value = m; localStorage.vlmNoteMode = m; if (m === 'read') load(); }
function onSaved(r) { if (note.value) { note.value.mtime = r.mtime; note.value.content = r.content; } }

// Make this note the scope (explicit action; browsing alone never changes the selection).
const topicInfo = computed(() => {
  for (const nb of store.notebooks) {
    const sec = nb.sections.find((s) => props.path.startsWith(s.path + '/'));
    if (allTopics(nb).some((t) => t.path === props.path)) return { nb: nb.path, section: sec?.path || null };
  }
  return null;
});
const isScope = computed(() => ns.value?.topic === props.path);
function useAsScope() {
  const t = topicInfo.value;
  if (!t) return;
  selectNotebook(t.nb);
  select({ section: t.section, topic: props.path });
  openTab(props.path);
  toast(`Scope set to “${name.value}”`, 'success');
}
</script>

<template>
  <div class="flex h-full">
    <div ref="scroller" class="min-w-0 flex-1 overflow-y-auto" @scroll.passive="onScroll">
      <div class="mx-auto max-w-3xl px-8 pt-6 pb-24">
        <div class="mb-4 flex items-center gap-2">
          <div class="min-w-0 flex-1 truncate text-xs text-zinc-400">{{ crumbs.join(' / ') }}</div>
          <span v-if="mode === 'edit'" class="flex items-center gap-1 text-[11px] text-zinc-400">
            <Loader2 v-if="saveStatus === 'saving'" class="size-3 animate-spin" /><Check v-else-if="saveStatus === 'saved'" class="size-3 text-emerald-500" />
            <span v-else class="size-1.5 rounded-full" :class="saveStatus === 'conflict' ? 'bg-red-500' : 'bg-amber-400'" />
            {{ { saved: 'Saved', dirty: 'Editing…', saving: 'Saving…', conflict: 'Conflict' }[saveStatus] }}
          </span>
          <button v-if="topicInfo && !isScope" class="btn-ghost !py-1 text-xs" title="Make this topic the chat & studio scope" @click="useAsScope"><Crosshair class="size-3.5" /> Use as scope</button>
          <span v-else-if="isScope" class="chip bg-accent-500/10 text-accent-600 dark:text-accent-400"><Crosshair class="size-3" /> current scope</span>
          <div class="flex rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800/80">
            <button class="flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium" :class="mode === 'edit' ? 'bg-white shadow-sm dark:bg-zinc-700' : 'text-zinc-500'" @click="setMode('edit')"><PencilLine class="size-3.5" /> Edit</button>
            <button class="flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium" :class="mode === 'read' ? 'bg-white shadow-sm dark:bg-zinc-700' : 'text-zinc-500'" @click="setMode('read')"><Eye class="size-3.5" /> Read</button>
          </div>
          <button v-if="note?.properties?.original" class="btn-ghost !py-1 text-xs" :disabled="reconverting" title="Convert the original file again with the latest converter" @click="reconvert">
            <Loader2 v-if="reconverting" class="size-3.5 animate-spin" /><RefreshCcw v-else class="size-3.5" /> Re-convert</button>
          <button v-if="isSource" class="btn-ghost !py-1 text-xs hover:!text-red-600" title="Remove this source" @click="removeSource"><Trash2 class="size-3.5" /> Remove</button>
          <button class="btn-ghost !p-1.5" title="Open in Obsidian" @click="openInObsidian(path)"><ExternalLink class="size-3.5" /></button>
        </div>
        <div v-if="error" class="text-sm text-red-500">{{ error }}</div>
        <div v-else-if="!note" class="space-y-3"><div class="skeleton h-7 w-1/2" /><div class="skeleton h-4" /><div class="skeleton h-4 w-5/6" /></div>
        <template v-else>
          <audio v-if="isAudio" controls :src="audioSrc" class="mb-4 w-full" />
          <Editor v-if="mode === 'edit'" ref="editor" :path="path" :content="note.content" :mtime="note.mtime" @saved="onSaved" @status="(s) => (saveStatus = s)" @selection="onSelection" />
          <article v-else ref="body" class="prose-note" v-html="html" />
          <SelectionToolbar v-if="sel && mode === 'edit' && ns?.active === path" :sel="sel" :path="path" @replace="replaceSel" @close="sel = null" />
        </template>
      </div>
    </div>
    <NoteSidePanel v-if="note" :note="note" :path="path" @heading="scrollToHeading" @reload="load" @insert="insertText" />
  </div>
</template>
