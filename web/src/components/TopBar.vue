<script setup>
import { MessageSquare, X, Search, Moon, Sun, PanelRight, FileText, Network, BookOpen } from 'lucide-vue-next';
import { store, ns, currentNb, closeTab, toggleDark, openGraph } from '../store.js';

const name = (p) => p.split('/').pop().replace(/\.md$/, '');
// A tab whose note lives in a different notebook is labeled with that notebook.
const otherNb = (p) => (currentNb.value && !p.startsWith(currentNb.value.path + '/') ? store.notebooks.find((n) => p.startsWith(n.path + '/'))?.name || 'vault' : null);
const icon = (t) => (t.kind === 'chat' ? MessageSquare : t.kind === 'graph' ? Network : FileText);
const label = (t) => (t.kind === 'chat' ? 'Chat' : t.kind === 'graph' ? 'Graph' : name(t.path));
function onAux(e, t) { if (e.button === 1) closeTab(t.id); }
</script>

<template>
  <header class="flex h-14 shrink-0 items-center gap-2 border-b border-zinc-200/80 px-3 dark:border-zinc-800">
    <div v-if="currentNb" class="flex shrink-0 items-center gap-1.5 pr-1 text-sm font-semibold text-zinc-500">
      <BookOpen class="size-4" /><span class="max-w-32 truncate">{{ currentNb.name }}</span>
      <span class="ml-1 h-5 w-px bg-zinc-200 dark:bg-zinc-800" />
    </div>
    <div v-if="store.reviewOpen" class="flex flex-1 items-center gap-2 text-sm font-semibold"><span class="text-zinc-400">·</span> Review dashboard
      <button class="btn-ghost ml-2 !py-1 text-xs font-normal" @click="store.reviewOpen = false">Back to {{ currentNb?.name || 'notebook' }}</button>
    </div>
    <div v-else-if="ns" class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none]">
      <div v-for="t in ns.tabs" :key="t.id" class="group flex shrink-0 items-center rounded-lg text-sm transition" @auxclick="onAux($event, t)"
        :class="ns.active === t.id ? 'bg-zinc-900 text-white dark:bg-white dark:text-zinc-900' : 'text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800'">
        <button class="flex max-w-48 items-center gap-1.5 py-1.5 pl-3" :class="t.kind === 'chat' && 'pr-3'" @click="ns.active = t.id" :title="t.path">
          <component :is="icon(t)" class="size-3.5 shrink-0" />
          <span class="truncate">{{ label(t) }}</span>
          <span v-if="t.path && otherNb(t.path)" class="shrink-0 text-[10px] opacity-60">· {{ otherNb(t.path) }}</span>
        </button>
        <button v-if="t.kind !== 'chat'" class="mr-1 ml-1 rounded p-0.5 opacity-40 group-hover:opacity-100" @click="closeTab(t.id)"><X class="size-3" /></button>
      </div>
    </div>
    <div v-else class="flex-1" />

    <button v-if="ns" class="btn-ghost shrink-0 !p-2" :class="ns.active === 'graph' && 'text-accent-600 dark:text-accent-400'" title="Notebook graph" @click="openGraph"><Network class="size-4" /></button>
    <button class="btn-outline shrink-0 !py-1 text-zinc-500" @click="store.paletteOpen = true">
      <Search class="size-3.5" /><span class="hidden xl:inline">Search…</span><kbd class="text-[10px] text-zinc-400">⌘K</kbd>
    </button>
    <button class="btn-ghost shrink-0 !p-2" title="Toggle theme" @click="toggleDark"><Sun v-if="store.dark" class="size-4" /><Moon v-else class="size-4" /></button>
    <button class="btn-ghost shrink-0 !p-2" :class="store.studioOpen && 'text-accent-600 dark:text-accent-400'" title="Studio (⌘.)" @click="store.studioOpen = !store.studioOpen"><PanelRight class="size-4" /></button>
  </header>
</template>
