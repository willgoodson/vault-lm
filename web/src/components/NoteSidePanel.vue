<script setup>
import { ref, watch, onMounted } from 'vue';
import { ListTree, ArrowUpRight, ArrowDownLeft, Tag, Sparkles, Link2, Unlink, Loader2, Plus, SlidersHorizontal } from 'lucide-vue-next';
import { store, openTab, toast } from '../store.js';
import { api } from '../api.js';

const props = defineProps({ note: Object, path: String });
const emit = defineEmits(['heading', 'reload', 'insert']);
const name = (p) => p.split('/').pop().replace(/\.md$/, '');
const notebookOf = () => store.notebooks.find((n) => props.path.startsWith(n.path + '/'))?.path;

const related = ref([]);
const mentions = ref([]);
const suggestions = ref(null);
const suggesting = ref(false);

async function loadExtras() {
  const q = `path=${encodeURIComponent(props.path)}`;
  const nb = notebookOf();
  [related.value, mentions.value] = await Promise.all([
    api(`/api/note/related?${q}${nb ? `&notebook=${encodeURIComponent(nb)}` : ''}`).catch(() => []),
    api(`/api/note/mentions?${q}`).catch(() => []),
  ]);
}
onMounted(loadExtras);
watch(() => props.note?.mtime, loadExtras);

async function suggest() {
  suggesting.value = true;
  try { suggestions.value = await api('/api/note/suggest-links', { path: props.path, notebook: notebookOf() }); }
  catch (e) { toast(e.message, 'error'); }
  suggesting.value = false;
}
function insertLink(s) {
  emit('insert', `[[${s.note}]]`);
  suggestions.value = suggestions.value.filter((x) => x.path !== s.path);
}
async function linkMention(m) {
  try {
    await api('/api/note/link-mention', { in: m.path, line: m.line, target: props.path });
    mentions.value = mentions.value.filter((x) => x !== m);
    toast(`Linked from “${name(m.path)}”`, 'success');
    emit('reload');
  } catch (e) { toast(e.message, 'error'); }
}
const props_ = () => Object.entries(props.note.properties || {}).filter(([k]) => k !== 'tags');
const fmt = (v) => (Array.isArray(v) ? v.join(', ') : v instanceof Object ? JSON.stringify(v) : String(v));
</script>

<template>
  <aside class="hidden w-72 shrink-0 overflow-y-auto border-l border-zinc-200/80 p-4 lg:block dark:border-zinc-800">
    <template v-if="props_().length">
      <div class="label mb-2 flex items-center gap-1.5"><SlidersHorizontal class="size-3" /> Properties</div>
      <div v-for="[k, v] in props_()" :key="k" class="flex gap-2 py-0.5 text-[13px]"><span class="w-24 shrink-0 truncate text-zinc-400">{{ k }}</span><span class="min-w-0 truncate">{{ fmt(v) }}</span></div>
    </template>
    <template v-if="note.tags.length">
      <div class="label mt-5 mb-2 flex items-center gap-1.5"><Tag class="size-3" /> Tags</div>
      <div class="flex flex-wrap gap-1">
        <button v-for="t in note.tags" :key="t" class="chip bg-zinc-100 text-zinc-600 hover:bg-accent-500/10 hover:text-accent-600 dark:bg-zinc-800 dark:text-zinc-300" @click="store.paletteQuery = '#' + t; store.paletteOpen = true">#{{ t }}</button>
      </div>
    </template>
    <template v-if="note.outline.length">
      <div class="label mt-5 mb-2 flex items-center gap-1.5"><ListTree class="size-3" /> Outline</div>
      <button v-for="(h, i) in note.outline" :key="i" class="block w-full truncate rounded py-0.5 text-left text-[13px] text-zinc-600 hover:text-accent-600 dark:text-zinc-400"
        :style="{ paddingLeft: `${(h.level - 1) * 10}px` }" @click="emit('heading', h.heading)">{{ h.heading }}</button>
    </template>

    <div class="label mt-5 mb-2 flex items-center gap-1.5"><ArrowUpRight class="size-3" /> Links</div>
    <p v-if="!note.links.length && !note.unresolved.length" class="text-xs text-zinc-400">None</p>
    <button v-for="l in note.links" :key="l" class="block w-full truncate rounded py-0.5 text-left text-[13px] text-accent-600 hover:underline dark:text-accent-400" @click="openTab(l)">{{ name(l) }}</button>
    <div v-for="u in note.unresolved" :key="u" class="truncate py-0.5 text-[13px] text-zinc-400 line-through decoration-dashed" title="Not created yet">{{ u }}</div>

    <div class="label mt-5 mb-2 flex items-center gap-1.5"><ArrowDownLeft class="size-3" /> Backlinks</div>
    <p v-if="!note.backlinks.length" class="text-xs text-zinc-400">None</p>
    <button v-for="l in note.backlinks" :key="l" class="block w-full truncate rounded py-0.5 text-left text-[13px] text-accent-600 hover:underline dark:text-accent-400" @click="openTab(l)">{{ name(l) }}</button>

    <template v-if="mentions.length">
      <div class="label mt-5 mb-2 flex items-center gap-1.5"><Unlink class="size-3" /> Unlinked mentions</div>
      <div v-for="(m, i) in mentions" :key="i" class="group mb-1.5 rounded-lg border border-zinc-200 p-2 dark:border-zinc-800">
        <div class="flex items-center gap-1">
          <button class="min-w-0 flex-1 truncate text-left text-xs font-medium hover:text-accent-600" @click="openTab(m.path)">{{ name(m.path) }}</button>
          <button class="chip bg-accent-500/10 text-accent-600 hover:bg-accent-500/20 dark:text-accent-400" @click="linkMention(m)"><Link2 class="size-2.5" /> Link</button>
        </div>
        <p class="mt-1 line-clamp-2 text-[11px] text-zinc-500">{{ m.text }}</p>
      </div>
    </template>

    <div class="mt-5 mb-2 flex items-center justify-between">
      <span class="label flex items-center gap-1.5"><Sparkles class="size-3" /> Related topics</span>
      <button class="text-[11px] font-medium text-accent-600 hover:underline disabled:opacity-50 dark:text-accent-400" :disabled="suggesting" @click="suggest">
        <Loader2 v-if="suggesting" class="inline size-3 animate-spin" /> Suggest links
      </button>
    </div>
    <template v-if="suggestions">
      <p v-if="!suggestions.length" class="mb-2 text-xs text-zinc-400">No new links to suggest.</p>
      <div v-for="s in suggestions" :key="s.path" class="mb-1.5 rounded-lg bg-accent-500/5 p-2">
        <div class="flex items-center gap-1">
          <button class="min-w-0 flex-1 truncate text-left text-xs font-medium hover:text-accent-600" @click="openTab(s.path)">{{ s.note }}</button>
          <button class="chip bg-accent-500/15 text-accent-700 hover:bg-accent-500/25 dark:text-accent-300" title="Insert a link at the cursor" @click="insertLink(s)"><Plus class="size-2.5" /> Link</button>
        </div>
        <p v-if="s.reason" class="mt-1 text-[11px] leading-snug text-zinc-500">{{ s.reason }}</p>
      </div>
    </template>
    <p v-if="!related.length" class="text-xs text-zinc-400">Nothing similar yet.</p>
    <button v-for="r in related" :key="r.path" class="flex w-full items-center gap-2 rounded py-0.5 text-left text-[13px] hover:text-accent-600" @click="openTab(r.path)">
      <span class="min-w-0 flex-1 truncate">{{ r.note }}</span>
      <span class="h-1 w-10 shrink-0 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800"><span class="block h-full rounded-full bg-accent-500/60" :style="{ width: `${Math.max(10, r.score * 100)}%` }" /></span>
    </button>
  </aside>
</template>
