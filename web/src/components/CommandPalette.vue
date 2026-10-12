<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import Fuse from 'fuse.js';
import { Search, FileText, Hash, CornerDownLeft, TextSearch, Tag, MessageSquare } from 'lucide-vue-next';
import { store, ns, openTab } from '../store.js';
import { api } from '../api.js';

const q = ref('');
const sel = ref(0);
const input = ref(null);
const fulltext = ref([]);
const notesOnly = computed(() => store.palette.filter((p) => !p.path.includes('/_studio/')));
const fuse = computed(() => new Fuse(notesOnly.value, { keys: [{ name: 'note', weight: 2 }, 'heading', 'path'], threshold: 0.38 }));

let timer;
watch(q, (v) => {
  sel.value = 0;
  clearTimeout(timer);
  if (v.trim().length < 2) { fulltext.value = []; return; }
  timer = setTimeout(async () => { fulltext.value = await api(`/api/search?q=${encodeURIComponent(v)}`).catch(() => []); }, 180);
});

const results = computed(() => {
  const v = q.value.trim();
  const name = (p) => p.split('/').pop().replace(/\.md$/, '');
  if (!v) return notesOnly.value.filter((p) => !p.heading).slice(0, 30).map((r) => ({ ...r, type: 'note' }));
  if (v.startsWith('#')) return fulltext.value.map((r) => ({ type: 'tag', path: r.path, note: name(r.path), detail: r.text }));
  const named = fuse.value.search(v, { limit: 20 }).map(({ item }) => ({ ...item, type: item.heading ? 'heading' : 'note' }));
  const seen = new Set(named.map((r) => r.path + '#' + r.heading));
  const text = fulltext.value.filter((r) => !seen.has(r.path + '#')).map((r) => ({ type: 'text', path: r.path, note: name(r.path), detail: r.text }));
  return [...named, ...text.slice(0, 15), { type: 'ask', note: `Ask “${v}” in chat`, path: '' }];
});

watch(() => store.paletteOpen, (o) => {
  if (o) { q.value = store.paletteQuery || ''; store.paletteQuery = ''; sel.value = 0; nextTick(() => input.value?.focus()); }
});

function go(item) {
  store.paletteOpen = false;
  if (item.type === 'ask') { store.composer = q.value; ns.value.active = 'chat'; return; }
  openTab(item.path, item.heading || undefined);
}
function onKey(e) {
  if (e.key === 'Escape') store.paletteOpen = false;
  else if (e.key === 'ArrowDown') { e.preventDefault(); sel.value = Math.min(sel.value + 1, results.value.length - 1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); sel.value = Math.max(sel.value - 1, 0); }
  else if (e.key === 'Enter' && results.value[sel.value]) go(results.value[sel.value]);
  nextTick(() => document.querySelector('[data-sel="true"]')?.scrollIntoView({ block: 'nearest' }));
}
const icons = { note: FileText, heading: Hash, text: TextSearch, tag: Tag, ask: MessageSquare };
</script>

<template>
  <Transition enter-from-class="opacity-0" leave-to-class="opacity-0" enter-active-class="transition duration-150" leave-active-class="transition duration-100">
    <div v-if="store.paletteOpen" class="fixed inset-0 z-50 flex items-start justify-center bg-zinc-900/30 pt-[15vh] backdrop-blur-sm" @mousedown.self="store.paletteOpen = false">
      <div class="panel w-full max-w-xl overflow-hidden shadow-2xl">
        <div class="flex items-center gap-3 border-b border-zinc-100 px-4 dark:border-zinc-800">
          <Search class="size-4 text-zinc-400" />
          <input ref="input" v-model="q" @keydown="onKey" class="h-12 flex-1 bg-transparent text-[15px] outline-none placeholder:text-zinc-400" placeholder="Search notes, headings and text, or #tag…" />
          <kbd class="rounded border border-zinc-200 px-1.5 text-[10px] text-zinc-400 dark:border-zinc-700">esc</kbd>
        </div>
        <ul class="max-h-96 overflow-y-auto p-1.5">
          <li v-for="(r, i) in results" :key="r.type + r.path + (r.heading || '') + i" :data-sel="i === sel">
            <button class="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left" :class="i === sel ? 'bg-accent-500/10' : ''" @mouseenter="sel = i" @click="go(r)">
              <component :is="icons[r.type]" class="size-4 shrink-0" :class="r.type === 'ask' ? 'text-accent-500' : 'text-zinc-400'" />
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm">{{ r.heading || r.note }}</div>
                <div v-if="r.type !== 'ask'" class="truncate text-[11px] text-zinc-400">{{ r.detail || (r.heading ? r.note + ' · ' + r.path : r.path) }}</div>
              </div>
              <CornerDownLeft v-if="i === sel" class="size-3.5 text-zinc-400" />
            </button>
          </li>
        </ul>
        <div class="flex gap-4 border-t border-zinc-100 px-4 py-2 text-[11px] text-zinc-400 dark:border-zinc-800">
          <span><kbd>↵</kbd> open</span><span><kbd>↑↓</kbd> navigate</span><span><kbd>#</kbd> filter by tag</span>
        </div>
      </div>
    </div>
  </Transition>
</template>
