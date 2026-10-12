<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
const emit = defineEmits(['done']);
import { Sparkles, Check, X, ArrowRight, RotateCcw } from 'lucide-vue-next';
import { scope, toast } from '../../store.js';
import { api } from '../../api.js';
import { render } from '../../lib/markdown.js';
import Generating from './Generating.vue';
import ScoreRing from './ScoreRing.vue';

// `initial` = preset questions (e.g. a weak-spot drill); `notebook` overrides where progress is logged.
const props = defineProps({ initial: Array, notebook: String, title: String });
const sc = props.initial ? { notebook: props.notebook } : scope();
const count = ref(5);
const difficulty = ref('medium');
const busy = ref(false);
const questions = ref([]);
const i = ref(0);
const picked = ref(null);
const answers = ref([]);
const q = computed(() => questions.value[i.value]);
const done = computed(() => questions.value.length && i.value >= questions.value.length);
const score = computed(() => answers.value.filter((a) => a.correct).length);

async function generate() {
  busy.value = true;
  try {
    const r = await api('/api/study/quiz', { ...sc, count: count.value, difficulty: difficulty.value });
    begin(r.questions);
  } catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
function begin(qs) { questions.value = qs; i.value = 0; picked.value = null; answers.value = []; }
function choose(n) {
  if (picked.value !== null || !q.value) return;
  picked.value = n;
  answers.value.push({ q: q.value, picked: n, correct: n === q.value.answer });
  api('/api/progress', { kind: 'quiz', notebook: props.notebook || sc.notebook, concept: q.value.cite || '', correct: n === q.value.answer, label: q.value.question }).catch(() => {});
}
function next() { i.value++; picked.value = null; }
const retryMissed = () => begin(answers.value.filter((a) => !a.correct).map((a) => a.q));

function onKey(e) {
  if (!q.value || ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName)) return;
  if (['1', '2', '3', '4'].includes(e.key)) choose(+e.key - 1);
  if (e.key === 'Enter' && picked.value !== null) next();
}
onMounted(() => { document.addEventListener('keydown', onKey); if (props.initial?.length) begin(props.initial); });
onUnmounted(() => document.removeEventListener('keydown', onKey));

const style = (n) => {
  if (picked.value === null) return 'hover:border-accent-400 hover:bg-accent-500/5';
  if (n === q.value.answer) return 'border-emerald-500 bg-emerald-500/10';
  if (n === picked.value) return 'border-red-500 bg-red-500/10';
  return 'opacity-50';
};
</script>

<template>
  <div class="p-4">
    <Generating v-if="busy" label="Writing your quiz…" />

    <div v-else-if="!questions.length" class="panel p-4">
      <div class="text-sm font-medium">New quiz</div>
      <p class="mt-0.5 text-xs text-zinc-500">Multiple choice. Every answer is explained and cited.</p>
      <div class="label mt-4 mb-1.5">Questions</div>
      <div class="flex gap-1.5"><button v-for="n in [5, 10, 15]" :key="n" class="btn-outline flex-1" :class="count === n && '!border-accent-500 text-accent-600'" @click="count = n">{{ n }}</button></div>
      <div class="label mt-4 mb-1.5">Difficulty</div>
      <div class="flex gap-1.5"><button v-for="d in ['easy', 'medium', 'hard']" :key="d" class="btn-outline flex-1 capitalize" :class="difficulty === d && '!border-accent-500 text-accent-600'" @click="difficulty = d">{{ d }}</button></div>
      <button class="btn-primary mt-5 w-full !py-2.5" @click="generate"><Sparkles class="size-4" /> Start quiz</button>
    </div>

    <div v-else-if="done" class="flex flex-col items-center text-center">
      <ScoreRing :value="score" :max="questions.length" :size="120" class="mt-4" />
      <div class="mt-3 text-lg font-semibold">{{ score === questions.length ? 'Perfect!' : score / questions.length >= 0.7 ? 'Nice work' : 'Keep going' }}</div>
      <div class="mt-5 w-full space-y-2 text-left">
        <div v-for="(a, k) in answers" :key="k" class="panel flex gap-2 p-3 text-sm">
          <component :is="a.correct ? Check : X" class="mt-0.5 size-4 shrink-0" :class="a.correct ? 'text-emerald-500' : 'text-red-500'" />
          <div class="min-w-0"><div>{{ a.q.question }}</div><div v-if="!a.correct" class="mt-1 text-xs text-zinc-500">Answer: {{ a.q.choices[a.q.answer] }}</div></div>
        </div>
      </div>
      <div class="mt-4 flex w-full gap-2">
        <button v-if="score < questions.length" class="btn-outline flex-1" @click="retryMissed"><RotateCcw class="size-3.5" /> Retry missed</button>
        <button v-if="initial" class="btn-primary flex-1" @click="emit('done')">Done</button>
        <button v-else class="btn-primary flex-1" @click="questions = []"><Sparkles class="size-3.5" /> New quiz</button>
      </div>
    </div>

    <div v-else>
      <div class="mb-3 flex items-center gap-2 text-xs text-zinc-400">
        <span>Question {{ i + 1 }} of {{ questions.length }}</span>
        <div class="ml-auto flex gap-1"><span v-for="(_, k) in questions" :key="k" class="size-1.5 rounded-full"
          :class="k < answers.length ? (answers[k].correct ? 'bg-emerald-500' : 'bg-red-500') : k === i ? 'bg-accent-500' : 'bg-zinc-200 dark:bg-zinc-700'" /></div>
      </div>
      <div class="text-[17px] leading-snug font-semibold">{{ q.question }}</div>
      <div class="mt-4 space-y-2">
        <button v-for="(c, n) in q.choices" :key="n" class="flex w-full items-start gap-3 rounded-xl border border-zinc-200 p-3 text-left text-sm transition dark:border-zinc-700" :class="style(n)" @click="choose(n)">
          <span class="grid size-6 shrink-0 place-items-center rounded-md bg-zinc-100 text-xs font-semibold dark:bg-zinc-800">{{ 'ABCD'[n] }}</span>
          <span class="pt-0.5">{{ c }}</span>
        </button>
      </div>
      <Transition enter-from-class="opacity-0 translate-y-1" enter-active-class="transition">
        <div v-if="picked !== null" class="mt-4 rounded-xl p-3.5 text-sm" :class="picked === q.answer ? 'bg-emerald-500/10' : 'bg-red-500/10'">
          <div class="font-semibold" :class="picked === q.answer ? 'text-emerald-700 dark:text-emerald-300' : 'text-red-700 dark:text-red-300'">{{ picked === q.answer ? 'Correct' : 'Not quite' }}</div>
          <p class="mt-1 text-zinc-600 dark:text-zinc-300">{{ q.explanation }}</p>
          <div v-if="q.cite" class="prose-note mt-1.5" v-html="render(q.cite)" />
        </div>
      </Transition>
      <button v-if="picked !== null" class="btn-primary mt-4 w-full !py-2.5" @click="next">{{ i + 1 < questions.length ? 'Next' : 'See results' }} <ArrowRight class="size-4" /></button>
      <p class="mt-3 text-center text-[11px] text-zinc-400">Press 1–4 to answer, Enter for the next question</p>
    </div>
  </div>
</template>
