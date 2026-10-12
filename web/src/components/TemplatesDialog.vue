<script setup>
import { ref, watch, computed } from 'vue';
import { X, Plus, Trash2, LayoutTemplate, RotateCcw } from 'lucide-vue-next';
import { store, toast } from '../store.js';
import { api } from '../api.js';

const props = defineProps({ open: Boolean });
const emit = defineEmits(['close']);
const list = ref([]);
const sel = ref(0);
const cur = computed(() => list.value[sel.value]);
watch(() => props.open, (o) => { if (o) { list.value = JSON.parse(JSON.stringify(store.templates)); sel.value = 0; } });

function add() { list.value.push({ id: '', name: 'New template', body: '# {{title}}\n\n' }); sel.value = list.value.length - 1; }
function remove(i) { list.value.splice(i, 1); sel.value = Math.max(0, Math.min(sel.value, list.value.length - 1)); }
async function save() {
  try { store.templates = await api('/api/templates', list.value, 'PUT'); toast('Templates saved', 'success'); emit('close'); }
  catch (e) { toast(e.message, 'error'); }
}
async function reset() {
  if (!confirm('Replace your templates with the built-in set?')) return;
  list.value = await api('/api/templates/reset', {});
  store.templates = list.value; sel.value = 0;
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fixed inset-0 z-50 grid place-items-center bg-zinc-900/30 backdrop-blur-sm" @mousedown.self="emit('close')">
      <div class="panel flex h-[560px] w-[820px] flex-col overflow-hidden shadow-2xl">
        <div class="flex items-center gap-2 border-b border-zinc-100 px-5 py-3.5 dark:border-zinc-800">
          <LayoutTemplate class="size-4 text-accent-500" />
          <div class="font-semibold">Topic templates</div>
          <div class="text-xs text-zinc-400">Stored in Vault-LM for this vault. Variables: <code v-pre>{{title}} {{date}} {{time}} {{notebook}} {{section}}</code></div>
          <button class="btn-ghost ml-auto !p-1" @click="emit('close')"><X class="size-4" /></button>
        </div>
        <div class="flex min-h-0 flex-1">
          <div class="w-56 shrink-0 overflow-y-auto border-r border-zinc-100 p-2 dark:border-zinc-800">
            <div v-for="(t, i) in list" :key="i" class="group flex items-center rounded-lg" :class="i === sel ? 'bg-accent-500/10 text-accent-700 dark:text-accent-300' : 'hover:bg-zinc-100 dark:hover:bg-zinc-800'">
              <button class="min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-sm" @click="sel = i">{{ t.name || 'Untitled' }}</button>
              <button class="mr-1 hidden rounded p-1 text-zinc-400 group-hover:block hover:text-red-600" @click="remove(i)"><Trash2 class="size-3" /></button>
            </div>
            <button class="menu-item mt-1 text-zinc-500" @click="add"><Plus class="size-3.5" /> New template</button>
          </div>
          <div v-if="cur" class="flex min-w-0 flex-1 flex-col gap-3 p-4">
            <input v-model="cur.name" class="input !py-1.5 font-medium" placeholder="Template name" />
            <textarea v-model="cur.body" class="input min-h-0 flex-1 resize-none font-mono text-[13px] leading-relaxed" spellcheck="false" />
          </div>
          <div v-else class="grid flex-1 place-items-center text-sm text-zinc-400">No templates</div>
        </div>
        <div class="flex items-center gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800">
          <button class="btn-ghost text-xs" @click="reset"><RotateCcw class="size-3.5" /> Restore built-ins</button>
          <button class="btn-ghost ml-auto" @click="emit('close')">Cancel</button>
          <button class="btn-primary" @click="save">Save templates</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
