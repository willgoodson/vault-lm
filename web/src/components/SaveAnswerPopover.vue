<script setup>
import { ref, watch, computed } from 'vue';
import { FilePlus2, Loader2 } from 'lucide-vue-next';
import Popover from './Popover.vue';
import { store, currentNb, selection, allTopics, openTab, refreshFiles, toast } from '../store.js';
import { api } from '../api.js';

const props = defineProps({ anchor: Object, open: Boolean, msg: Object });
const emit = defineEmits(['close']);
const title = ref('');
const folder = ref('');
const appendTo = ref('');
const busy = ref(false);
const suggesting = ref(false);

const folders = computed(() => (currentNb.value ? [{ path: currentNb.value.path, name: `📓 ${currentNb.value.name}` }, ...currentNb.value.sections.map((s) => ({ path: s.path, name: `# ${s.name}` }))] : []));
const topics = computed(() => allTopics(currentNb.value));

watch(() => props.open, async (o) => {
  if (!o) return;
  folder.value = selection.value.section || selection.value.notebook;
  appendTo.value = '';
  title.value = '';
  suggesting.value = true;
  title.value = (await api('/api/chat/title', { question: props.msg.q, answer: props.msg.a }).catch(() => ({ title: '' }))).title;
  suggesting.value = false;
}, { immediate: true }); // the popover mounts already open

async function save() {
  busy.value = true;
  try {
    const r = await api('/api/topic/from-answer', { folder: folder.value, title: title.value || 'Saved answer', question: props.msg.q, answer: props.msg.a, sources: props.msg.sources, appendTo: appendTo.value || undefined });
    await refreshFiles();
    openTab(r.path);
    toast(appendTo.value ? 'Added to the topic' : 'Saved as a new topic', 'success');
    emit('close');
  } catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
</script>

<template>
  <Popover :anchor="anchor" :open="open" :width="340" align="end" @close="emit('close')">
    <div class="space-y-2.5 p-1.5">
      <div class="text-sm font-semibold">Save answer to {{ currentNb?.name }}</div>
      <div>
        <label class="label">{{ appendTo ? 'Heading' : 'Title' }}</label>
        <div class="relative mt-1">
          <input v-model="title" class="input !py-1.5" :placeholder="suggesting ? 'Suggesting a title…' : 'Title'" />
          <Loader2 v-if="suggesting" class="absolute top-2 right-2.5 size-3.5 animate-spin text-zinc-400" />
        </div>
      </div>
      <div>
        <label class="label">Save as</label>
        <select v-model="appendTo" class="input mt-1 !py-1.5">
          <option value="">A new topic</option>
          <option v-for="t in topics" :key="t.path" :value="t.path">Add to “{{ t.name }}”</option>
        </select>
      </div>
      <div v-if="!appendTo">
        <label class="label">In</label>
        <select v-model="folder" class="input mt-1 !py-1.5">
          <option v-for="f in folders" :key="f.path" :value="f.path">{{ f.name }}</option>
        </select>
      </div>
      <button class="btn-primary w-full" :disabled="busy || (!folder && !appendTo)" @click="save"><FilePlus2 class="size-3.5" /> {{ appendTo ? 'Append to topic' : 'Create topic' }}</button>
      <p class="text-[11px] text-zinc-400">Keeps the question, the answer and its citations as [[links]].</p>
    </div>
  </Popover>
</template>
