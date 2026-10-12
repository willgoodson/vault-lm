<script setup>
// First launch (no vault yet): open an existing folder (an Obsidian vault works as-is) or create one.
import { ref } from 'vue';
import { FolderOpen, FolderPlus, Vault, Loader2 } from 'lucide-vue-next';
import { store, loadAll, connectEvents, toast } from '../store.js';
import { api } from '../api.js';
import { pickFolder, pickNewFolder } from '../native.js';

const busy = ref(false);
async function go(kind, p) {
  if (!p) return;
  busy.value = true;
  try { await api(kind === 'open' ? '/api/vaults/select' : '/api/vaults/create', { path: p }); await loadAll(); toast(`Opened ${store.info.vaultName}`, 'success'); }
  catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
const suggestions = ref([]);
api('/api/vaults').then((v) => (suggestions.value = v.slice(0, 5))).catch(() => {});
</script>

<template>
  <div class="grid h-full place-items-center bg-zinc-50 dark:bg-zinc-950">
    <div class="w-[460px] text-center">
      <img src="/logo.svg" alt="" class="mx-auto size-24 drop-shadow-xl" />
      <h1 class="mt-5 text-2xl font-semibold tracking-tight">Welcome to Vault-LM</h1>
      <p class="mt-1.5 text-sm text-zinc-500">Your notes and a study partner that runs on your computer. Choose a folder of Markdown notes (an Obsidian vault works as-is) or start a new one.</p>
      <div class="mt-6 grid grid-cols-2 gap-3">
        <button class="panel flex flex-col items-center gap-2 p-5 transition hover:-translate-y-0.5 hover:shadow-md" :disabled="busy" @click="pickFolder('Open a folder as a vault').then((p) => go('open', p))">
          <FolderOpen class="size-6 text-accent-500" /><span class="text-sm font-medium">Open a folder</span>
        </button>
        <button class="panel flex flex-col items-center gap-2 p-5 transition hover:-translate-y-0.5 hover:shadow-md" :disabled="busy" @click="pickNewFolder('Create a new vault').then((p) => go('create', p))">
          <FolderPlus class="size-6 text-accent-500" /><span class="text-sm font-medium">Create a vault</span>
        </button>
      </div>
      <div v-if="suggestions.length" class="mt-6 text-left">
        <div class="label mb-1.5">Found on this computer</div>
        <button v-for="v in suggestions" :key="v.path" class="menu-item" :disabled="busy" @click="go('open', v.path)">
          <Vault class="size-3.5 shrink-0 text-zinc-400" /><span class="truncate">{{ v.name }}</span>
          <span class="ml-auto truncate text-[11px] text-zinc-400">{{ v.path.replace(/^\/Users\/[^/]+/, '~') }}</span>
        </button>
      </div>
      <Loader2 v-if="busy" class="mx-auto mt-4 size-5 animate-spin text-zinc-400" />
    </div>
  </div>
</template>
