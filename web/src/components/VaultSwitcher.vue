<script setup>
import { ref } from 'vue';
import { Vault, ChevronsUpDown, Check, FolderOpen, FolderPlus, Loader2 } from 'lucide-vue-next';
import { onClickOutside } from '../lib/clickOutside.js';
import { store, loadAll, toast } from '../store.js';
import { api } from '../api.js';
import FolderBrowser from './FolderBrowser.vue';
import { isDesktop } from '../api.js';
import { pickFolder, pickNewFolder } from '../native.js';

const open = ref(false);
const busy = ref(false);
const root = ref(null);
const browser = ref(null); // 'open' | 'create'
onClickOutside(root, () => (open.value = false));

async function refresh() { store.vaults = await api('/api/vaults'); }
async function run(url, path, msg) {
  open.value = false; browser.value = null; busy.value = true;
  try { await api(url, { path }); await loadAll(); toast(msg(store.info.vaultName), 'success'); }
  catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
const select = (path) => run('/api/vaults/select', path, (n) => `Switched to ${n}`);
const create = (path) => run('/api/vaults/create', path, (n) => `Created ${n}. It's a plain folder, so Obsidian can open it too.`);
// Desktop: native folder dialogs instead of the in-app folder browser.
async function openFolderNative() { open.value = false; const p = await pickFolder('Open a folder as a vault'); if (p) select(p); }
async function createNative() { open.value = false; const p = await pickNewFolder('Create a new vault'); if (p) create(p); }
defineExpose({ openFolderNative, createNative });
</script>

<template>
  <div ref="root" class="relative">
    <button class="flex w-full items-center gap-2.5 rounded-xl border border-zinc-200 bg-white px-2.5 py-2 text-left shadow-sm transition hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700"
      @click="open = !open; open && refresh()">
      <div class="grid size-7 place-items-center rounded-lg bg-zinc-100 dark:bg-zinc-800">
        <Loader2 v-if="busy" class="size-3.5 animate-spin" /><Vault v-else class="size-3.5" />
      </div>
      <div class="min-w-0 flex-1">
        <div class="truncate text-sm font-medium">{{ store.info.vaultName || '…' }}</div>
        <div class="truncate text-[11px] text-zinc-400">{{ store.info.vault?.replace(/^\/Users\/[^/]+/, '~') }}</div>
      </div>
      <ChevronsUpDown class="size-4 text-zinc-400" />
    </button>

    <Transition enter-from-class="opacity-0 -translate-y-1" leave-to-class="opacity-0" enter-active-class="transition duration-150" leave-active-class="transition duration-100">
      <div v-if="open" class="panel absolute inset-x-0 top-full z-30 mt-1.5 p-1.5 shadow-xl">
        <div class="label px-2 pt-1 pb-1.5">Vaults</div>
        <button v-for="v in store.vaults" :key="v.path" class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-zinc-100 dark:hover:bg-zinc-800" @click="select(v.path)">
          <Check class="size-3.5 shrink-0" :class="v.current ? 'text-accent-500' : 'invisible'" />
          <div class="min-w-0 flex-1">
            <div class="truncate">{{ v.name }}</div>
            <div class="truncate text-[11px] text-zinc-400" :title="v.path">{{ v.path.replace(/^\/Users\/[^/]+/, '~') }}</div>
          </div>
          <span class="chip shrink-0" :class="v.indexed ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' : 'bg-zinc-100 text-zinc-400 dark:bg-zinc-800'">
            {{ v.indexed ? 'indexed' : v.fromObsidian ? 'in Obsidian' : 'new' }}
          </span>
        </button>
        <div class="my-1 h-px bg-zinc-100 dark:bg-zinc-800" />
        <button class="menu-item" @click="isDesktop ? openFolderNative() : (browser = 'open', open = false)"><FolderOpen class="size-3.5" /> Open folder as vault…</button>
        <button class="menu-item" @click="isDesktop ? createNative() : (browser = 'create', open = false)"><FolderPlus class="size-3.5" /> Create new vault…</button>
      </div>
    </Transition>
    <FolderBrowser :open="browser === 'open'" title="Open a folder as a vault" @close="browser = null" @pick="select" />
    <FolderBrowser :open="browser === 'create'" title="Create a new vault in…" action="Create" create @close="browser = null" @pick="create" />
  </div>
</template>
