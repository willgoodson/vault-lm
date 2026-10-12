<script setup>
// Edit app-side details for a notebook or section. Stored in Vault-LM's data folder, not the vault.
import { ref, watch, nextTick } from 'vue';
import { X, Plus, Trash2, Target } from 'lucide-vue-next';
import { api } from '../api.js';
import { toast, refreshFiles } from '../store.js';
import { COLORS } from '../lib/colors.js';

const props = defineProps({ item: Object }); // { path, name, kind, meta }
const emit = defineEmits(['close']);
const f = ref({});
const goalInput = ref(null);
watch(() => props.item, (it) => {
  if (!it) return;
  const m = it.meta || {};
  f.value = { description: m.description || '', goals: [...(m.goals || [])], status: m.status || 'active', due: m.due || '', color: m.color || '' };
  if (!f.value.goals.length) f.value.goals.push('');
}, { immediate: true });

function addGoal() { f.value.goals.push(''); nextTick(() => goalInput.value?.at?.(-1)?.focus()); }
async function save() {
  try {
    await api('/api/meta', { path: props.item.path, patch: { ...f.value, status: f.value.status === 'active' ? '' : f.value.status } }, 'PUT');
    await refreshFiles();
    toast('Details saved', 'success');
    emit('close');
  } catch (e) { toast(e.message, 'error'); }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="item" class="fixed inset-0 z-50 grid place-items-center bg-zinc-900/30 backdrop-blur-sm" @mousedown.self="emit('close')" @keydown.esc="emit('close')">
      <div class="panel w-[520px] overflow-hidden shadow-2xl">
        <div class="flex items-center gap-2 border-b border-zinc-100 px-5 py-3.5 dark:border-zinc-800">
          <div>
            <div class="font-semibold">{{ item.name }}</div>
            <div class="text-xs text-zinc-400">{{ item.kind === 'notebook' ? 'Notebook' : 'Section' }} details. These steer chat answers, overviews and the Studio.</div>
          </div>
          <button class="btn-ghost ml-auto !p-1" @click="emit('close')"><X class="size-4" /></button>
        </div>
        <div class="max-h-[65vh] space-y-4 overflow-y-auto px-5 py-4">
          <div>
            <label class="label">Description</label>
            <textarea v-model="f.description" rows="3" class="input mt-1.5 resize-y" placeholder="What is this for? e.g. “Studying for my structural engineering exam, with an emphasis on load calculations.”" />
          </div>
          <div>
            <label class="label flex items-center gap-1"><Target class="size-3" /> Goals</label>
            <div class="mt-1.5 space-y-1.5">
              <div v-for="(g, i) in f.goals" :key="i" class="flex gap-1.5">
                <input ref="goalInput" v-model="f.goals[i]" class="input !py-1.5" :placeholder="i === 0 ? 'e.g. Explain the main load types without notes' : 'Another goal'" @keydown.enter.prevent="addGoal" />
                <button class="btn-ghost !p-1.5" title="Remove" @click="f.goals.splice(i, 1)"><Trash2 class="size-3.5" /></button>
              </div>
              <button class="btn-ghost !px-2 text-xs" @click="addGoal"><Plus class="size-3.5" /> Add goal</button>
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="label">Status</label>
              <select v-model="f.status" class="input mt-1.5 !py-1.5">
                <option value="active">Active</option><option value="paused">Paused</option><option value="done">Done</option>
              </select>
            </div>
            <div>
              <label class="label">Due date</label>
              <input v-model="f.due" type="date" class="input mt-1.5 !py-1.5" />
            </div>
          </div>
          <div>
            <label class="label">Color</label>
            <div class="mt-1.5 flex gap-2">
              <button class="grid size-7 place-items-center rounded-full border border-dashed border-zinc-300 text-[10px] text-zinc-400 dark:border-zinc-600" :class="!f.color && 'ring-2 ring-accent-500 ring-offset-2 dark:ring-offset-zinc-900'" @click="f.color = ''">—</button>
              <button v-for="(hex, name) in COLORS" :key="name" class="size-7 rounded-full transition" :style="{ background: hex }" :title="name"
                :class="f.color === name && 'ring-2 ring-accent-500 ring-offset-2 dark:ring-offset-zinc-900'" @click="f.color = name" />
            </div>
          </div>
          <p class="text-[11px] text-zinc-400">Saved inside Vault-LM, not in your vault files.</p>
        </div>
        <div class="flex justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800">
          <button class="btn-ghost" @click="emit('close')">Cancel</button>
          <button class="btn-primary" @click="save">Save details</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
