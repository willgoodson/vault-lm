<script setup>
import { ref, computed } from 'vue';
import { Plus, FolderTree, Search, Sparkles, ChevronRight, Settings2, Loader2, GraduationCap } from 'lucide-vue-next';
import { store, currentNb, loadAll, toast } from '../store.js';
import { api } from '../api.js';
import VaultSwitcher from './VaultSwitcher.vue';
import NotebookTree from './NotebookTree.vue';
import FileTree from './FileTree.vue';
import TemplatesDialog from './TemplatesDialog.vue';

const tree = ref(null);
const filter = ref('');
const wholeVault = ref(false);
const showFiles = ref(localStorage.vlmShowFiles === '1');
const showLayout = ref(false);
const showTemplates = ref(false);
const hideDone = ref(localStorage.vlmHideDone === '1');
const toggleDone = () => { hideDone.value = !hideDone.value; localStorage.vlmHideDone = hideDone.value ? '1' : '0'; };
const toggleFiles = () => { showFiles.value = !showFiles.value; localStorage.vlmShowFiles = showFiles.value ? '1' : '0'; };

const findDir = (nodes, p) => {
  for (const n of nodes) {
    if (n.type !== 'dir') continue;
    if (n.path === p) return n;
    if (p.startsWith(n.path + '/')) return findDir(n.children, p);
  }
};
const nodes = computed(() => (wholeVault.value || !store.notebook ? store.tree : findDir(store.tree, store.notebook)?.children ?? []));

async function setRoot(root) {
  await api('/api/layout', { root }, 'PUT');
  await loadAll();
  showLayout.value = false;
  toast(root ? `Notebooks now come from “${root}/”` : 'Notebooks now come from the vault root', 'success');
}
</script>

<template>
  <aside class="flex w-72 shrink-0 flex-col border-r border-zinc-200/80 bg-white/60 backdrop-blur dark:border-zinc-800 dark:bg-zinc-900/40">
    <div class="flex items-center gap-2 px-4 pt-4 pb-3">
      <div class="grid size-8 place-items-center rounded-xl bg-gradient-to-br from-accent-500 to-fuchsia-500 text-white shadow-sm shadow-accent-500/30">
        <Sparkles class="size-4" />
      </div>
      <div class="font-semibold tracking-tight">Vault-LM</div>
    </div>

    <div class="px-3"><VaultSwitcher /></div>

    <div v-if="store.index.running || store.overview" class="mx-3 mt-2 rounded-lg bg-accent-500/10 px-3 py-2">
      <template v-if="store.index.running">
        <div class="flex justify-between text-[11px] font-medium text-accent-600 dark:text-accent-400">
          <span>Indexing…</span><span>{{ store.index.done }}/{{ store.index.total }}</span>
        </div>
        <div class="mt-1.5 h-1 overflow-hidden rounded-full bg-accent-500/15">
          <div class="h-full rounded-full bg-accent-500 transition-all" :style="{ width: `${(store.index.done / Math.max(1, store.index.total)) * 100}%` }" />
        </div>
      </template>
      <div v-else class="flex items-center gap-1.5 text-[11px] font-medium text-accent-600 dark:text-accent-400">
        <Loader2 class="size-3 animate-spin" /> Updating the {{ store.overview.split('/').pop() }} overview…
      </div>
    </div>

    <button class="mx-2 mt-3 flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm font-medium transition"
      :class="store.reviewOpen ? 'bg-accent-500/10 text-accent-700 dark:text-accent-300' : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800'"
      @click="store.reviewOpen = !store.reviewOpen">
      <GraduationCap class="size-4" /> Review
      <span v-if="store.dueTotal" class="chip ml-auto bg-accent-500 text-white">{{ store.dueTotal }} due</span>
    </button>

    <div class="mt-3 flex items-center justify-between px-4">
      <span class="label">Notebooks</span>
      <div class="-mr-2 flex">
        <button class="btn-ghost !p-1" title="Where notebooks live" @click="showLayout = !showLayout"><Settings2 class="size-3.5" /></button>
        <button class="btn-ghost !p-1" title="New notebook" @click="tree.startCreate(store.layout.root, 'notebook')"><Plus class="size-3.5" /></button>
      </div>
    </div>
    <div v-if="showLayout" class="panel mx-3 mt-1.5 p-2.5">
      <div class="text-xs font-medium">Notebooks are the folders inside:</div>
      <select class="input mt-1.5 !py-1.5 text-[13px]" :value="store.layout.root" @change="setRoot($event.target.value)">
        <option value="">Vault root</option>
        <option v-for="f in store.layout.folders" :key="f" :value="f">{{ f }}/</option>
      </select>
      <p class="mt-1.5 text-[11px] leading-snug text-zinc-400">Subfolders become <b>sections</b>, and notes become <b>topics</b>. This is saved for this vault only.</p>
      <label class="mt-2 flex cursor-pointer items-center gap-2 text-xs"><input type="checkbox" :checked="hideDone" class="accent-accent-500" @change="toggleDone" /> Hide notebooks and sections marked done</label>
      <button class="btn-outline mt-2.5 w-full !py-1 text-xs" @click="showTemplates = true">Manage topic templates…</button>
      <button class="btn-outline mt-1.5 w-full !py-1 text-xs" @click="store.modelsOpen = true">Models…</button>
      <TemplatesDialog :open="showTemplates" @close="showTemplates = false" />
    </div>

    <nav class="mt-1 min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      <NotebookTree ref="tree" :hide-done="hideDone" />
    </nav>

    <div class="border-t border-zinc-200/80 dark:border-zinc-800" :class="showFiles && 'flex max-h-[40%] min-h-48 flex-col'">
      <div class="flex items-center justify-between px-4 py-2.5">
        <button class="label flex items-center gap-1.5 hover:text-zinc-600" @click="toggleFiles">
          <ChevronRight class="size-3 transition" :class="showFiles && 'rotate-90'" /><FolderTree class="size-3" /> Vault files
        </button>
        <button v-if="showFiles" class="max-w-32 truncate text-[11px] font-medium text-zinc-400 hover:text-accent-500" @click="wholeVault = !wholeVault">
          {{ wholeVault || !currentNb ? 'Whole vault' : currentNb.name }}
        </button>
      </div>
      <template v-if="showFiles">
        <div class="relative mx-3">
          <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-zinc-400" />
          <input v-model="filter" class="input !rounded-lg !py-1.5 !pl-8 text-[13px]" placeholder="Filter files…" />
        </div>
        <div class="mt-1 min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          <FileTree :nodes="nodes" :filter="filter.toLowerCase()" />
        </div>
        <p class="px-4 pb-2 text-[10px] text-zinc-400">Browsing opens a tab. It doesn't change your selection.</p>
      </template>
    </div>
  </aside>
</template>
