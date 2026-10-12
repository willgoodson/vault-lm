<script setup>
import { ref } from 'vue';
import { FileText, HelpCircle, GraduationCap, CalendarClock, ExternalLink } from 'lucide-vue-next';
import { scope, toast, openTab } from '../../store.js';
import { api } from '../../api.js';
import { render } from '../../lib/markdown.js';
import Generating from './Generating.vue';

const sc = scope();
const TYPES = [
  { id: 'summary', label: 'Briefing', icon: FileText },
  { id: 'faq', label: 'FAQ', icon: HelpCircle },
  { id: 'study-guide', label: 'Study guide', icon: GraduationCap },
  { id: 'timeline', label: 'Timeline', icon: CalendarClock },
];
const busy = ref('');
const out = ref(null);
async function make(t) {
  busy.value = t.label;
  try { out.value = await api('/api/brief', { ...sc, type: t.id }); }
  catch (e) { toast(e.message, 'error'); }
  busy.value = '';
}
</script>

<template>
  <div class="p-4">
    <div class="grid grid-cols-2 gap-2">
      <button v-for="t in TYPES" :key="t.id" class="btn-outline !justify-start !py-2.5" :disabled="!!busy" @click="make(t)"><component :is="t.icon" class="size-4 text-accent-500" /> {{ t.label }}</button>
    </div>
    <Generating v-if="busy" :label="`Writing ${busy.toLowerCase()}…`" />
    <article v-else-if="out" class="panel mt-4 p-4">
      <button class="btn-ghost -mt-1 mb-2 -ml-2 text-xs" @click="openTab(out.path)"><ExternalLink class="size-3" /> {{ out.path }}</button>
      <div class="prose-note text-sm" v-html="render(out.body)" />
    </article>
  </div>
</template>
