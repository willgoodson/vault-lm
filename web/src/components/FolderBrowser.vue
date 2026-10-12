<script setup>
// Pick a folder on disk (server-side listing, since a web page can't show a native picker).
import { ref, watch } from 'vue';
import { Folder, ArrowUp, Vault, X } from 'lucide-vue-next';
import { api } from '../api.js';

const props = defineProps({ open: Boolean, title: String, action: { type: String, default: 'Open' }, create: Boolean });
const emit = defineEmits(['close', 'pick']);
const cur = ref(null);
const name = ref('');
const err = ref('');
async function go(path) {
  err.value = '';
  try { cur.value = await api(`/api/fs/list?path=${encodeURIComponent(path)}`); } catch (e) { err.value = e.message; }
}
watch(() => props.open, (o) => { if (o) { name.value = ''; go(cur.value?.path || '~/Documents'); } });
const pick = () => emit('pick', props.create ? `${cur.value.path}/${name.value.trim()}` : cur.value.path);
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fixed inset-0 z-50 grid place-items-center bg-zinc-900/30 backdrop-blur-sm" @mousedown.self="emit('close')">
      <div class="panel flex h-[480px] w-[520px] flex-col overflow-hidden shadow-2xl">
        <div class="flex items-center gap-2 border-b border-zinc-100 px-4 py-3 dark:border-zinc-800">
          <div class="font-semibold">{{ title }}</div>
          <button class="btn-ghost ml-auto !p-1" @click="emit('close')"><X class="size-4" /></button>
        </div>
        <div class="flex items-center gap-2 px-4 py-2">
          <button class="btn-outline !p-1.5" :disabled="!cur?.parent" @click="go(cur.parent)"><ArrowUp class="size-3.5" /></button>
          <div class="truncate text-xs text-zinc-500" :title="cur?.path">{{ cur?.path?.replace(/^\/Users\/[^/]+/, '~') }}</div>
        </div>
        <ul class="min-h-0 flex-1 overflow-y-auto px-2">
          <li v-for="d in cur?.dirs" :key="d.path">
            <button class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-zinc-100 dark:hover:bg-zinc-800" @click="go(d.path)">
              <component :is="d.isVault ? Vault : Folder" class="size-4 shrink-0" :class="d.isVault ? 'text-accent-500' : 'text-zinc-400'" />
              <span class="truncate">{{ d.name }}</span>
              <span v-if="d.isVault" class="chip ml-auto bg-accent-500/10 text-accent-600">vault</span>
            </button>
          </li>
          <li v-if="cur && !cur.dirs.length" class="px-2 py-6 text-center text-xs text-zinc-400">No subfolders</li>
          <li v-if="err" class="px-2 py-2 text-xs text-red-500">{{ err }}</li>
        </ul>
        <div class="flex items-center gap-2 border-t border-zinc-100 p-3 dark:border-zinc-800">
          <input v-if="create" v-model="name" class="input flex-1" placeholder="New vault name" @keydown.enter="name.trim() && pick()" />
          <div v-else class="flex-1 truncate text-xs text-zinc-500">Open <b>{{ cur?.path.split('/').pop() }}</b> as a vault</div>
          <button class="btn-primary" :disabled="!cur || (create && !name.trim())" @click="pick">{{ action }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
