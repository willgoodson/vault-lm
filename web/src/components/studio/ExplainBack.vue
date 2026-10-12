<script setup>
import { ref, computed } from 'vue';
import { Sparkles, CheckCircle2, CircleDashed, AlertTriangle, RotateCcw } from 'lucide-vue-next';
import { store, scope, toast } from '../../store.js';
import { api } from '../../api.js';
import { render } from '../../lib/markdown.js';
import Generating from './Generating.vue';
import ScoreRing from './ScoreRing.vue';

const sc = scope();
const topic = ref('');
const text = ref('');
const busy = ref(false);
const result = ref(null);

// Topic ideas from the headings in scope.
const ideas = computed(() => {
  const inScope = (p) => (sc.topic ? p.path === sc.topic : p.path.startsWith((sc.section || sc.notebook) + '/'));
  const hs = store.palette.filter((p) => inScope(p) && p.heading && !p.path.includes('/sources/')).map((p) => p.heading);
  return [...new Set(hs)].slice(0, 8);
});

async function submit() {
  busy.value = true;
  try { result.value = await api('/api/study/explain', { ...sc, subject: topic.value, explanation: text.value }); }
  catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
const sections = computed(() => result.value ? [
  { title: 'What you got right', items: result.value.gotRight, icon: CheckCircle2, cls: 'text-emerald-500' },
  { title: 'Gaps to fill', items: result.value.gaps, icon: CircleDashed, cls: 'text-amber-500' },
  { title: 'Misconceptions', items: result.value.misconceptions, icon: AlertTriangle, cls: 'text-red-500' },
].filter((s) => s.items?.length) : []);
</script>

<template>
  <div class="p-4">
    <Generating v-if="busy" label="Comparing against your notes…" />

    <div v-else-if="result">
      <div class="flex items-center gap-4">
        <ScoreRing :value="Math.round(result.score)" :max="10" />
        <div class="min-w-0 flex-1">
          <div class="label">{{ topic }}</div>
          <p class="mt-1 text-sm leading-snug">{{ result.summary }}</p>
        </div>
      </div>
      <div v-for="s in sections" :key="s.title" class="mt-5">
        <div class="mb-2 flex items-center gap-1.5 text-sm font-semibold"><component :is="s.icon" class="size-4" :class="s.cls" /> {{ s.title }}</div>
        <ul class="space-y-1.5">
          <li v-for="(it, k) in s.items" :key="k" class="panel px-3 py-2 text-sm">{{ it }}</li>
        </ul>
      </div>
      <div v-if="result.cites?.length" class="prose-note mt-4" v-html="render(result.cites.join(' '), result.sources)" />
      <div class="mt-5 flex gap-2">
        <button class="btn-outline flex-1" @click="result = null"><RotateCcw class="size-3.5" /> Try again</button>
        <button class="btn-primary flex-1" @click="result = null; text = ''; topic = ''">New topic</button>
      </div>
    </div>

    <div v-else>
      <div class="rounded-xl bg-amber-500/10 p-3 text-xs leading-relaxed text-amber-800 dark:text-amber-200">
        <strong>Feynman technique:</strong> explain a topic in plain words, as if you were teaching it. You'll get a grade against your notes and see what you missed.
      </div>
      <div class="label mt-4 mb-1.5">Topic</div>
      <input v-model="topic" class="input" placeholder="e.g. Astrophage" />
      <div v-if="ideas.length && !topic" class="mt-2 flex flex-wrap gap-1.5">
        <button v-for="t in ideas" :key="t" class="chip bg-zinc-100 py-1 text-zinc-600 hover:bg-accent-500/10 hover:text-accent-600 dark:bg-zinc-800 dark:text-zinc-300" @click="topic = t">{{ t }}</button>
      </div>
      <div class="label mt-4 mb-1.5">Your explanation</div>
      <textarea v-model="text" rows="8" class="input resize-y leading-relaxed" placeholder="Explain it in your own words…" />
      <div class="mt-1 text-right text-[11px] text-zinc-400">{{ text.trim().split(/\s+/).filter(Boolean).length }} words</div>
      <button class="btn-primary mt-3 w-full !py-2.5" :disabled="!topic.trim() || text.trim().length < 20" @click="submit"><Sparkles class="size-4" /> Grade my explanation</button>
    </div>
  </div>
</template>
