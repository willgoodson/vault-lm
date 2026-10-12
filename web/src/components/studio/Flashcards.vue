<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { Sparkles, Play, RotateCcw, ExternalLink, PartyPopper } from 'lucide-vue-next';
import { scope, toast, openTab } from '../../store.js';
import { api } from '../../api.js';
import { render } from '../../lib/markdown.js';
import Generating from './Generating.vue';

const sc = scope();
const deck = ref({ cards: [], due: 0 });
const deckPath = ref('');
const busy = ref(false);
const count = ref(15);
const queue = ref([]);
const flipped = ref(false);
const reviewing = ref(false);
const reviewed = ref(0);
const card = computed(() => queue.value[0]);

const load = async () => (deck.value = await api('/api/study/deck', sc));
onMounted(load);

async function generate() {
  busy.value = true;
  try {
    const r = await api('/api/study/flashcards', { ...sc, count: count.value });
    deck.value = r; deckPath.value = r.path;
    toast(`Deck has ${r.cards.length} cards`, 'success');
  } catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}

function start(all = false) {
  const now = Date.now();
  queue.value = [...deck.value.cards].filter((c) => all || c.due <= now).sort(() => Math.random() - 0.5);
  reviewed.value = 0; flipped.value = false; reviewing.value = true;
}

async function grade(g) {
  const c = card.value;
  if (!c || !flipped.value) return;
  await api('/api/study/review', { id: c.id, grade: g });
  queue.value.shift();
  if (g === 0) queue.value.push(c);
  reviewed.value++;
  flipped.value = false;
  if (!queue.value.length) await load();
}

function onKey(e) {
  if (!reviewing.value || ['INPUT', 'TEXTAREA'].includes(e.target.tagName)) return;
  if (e.code === 'Space') { e.preventDefault(); flipped.value = !flipped.value; }
  if (['1', '2', '3', '4'].includes(e.key)) grade(+e.key - 1);
}
onMounted(() => document.addEventListener('keydown', onKey));
onUnmounted(() => document.removeEventListener('keydown', onKey));

const GRADES = [
  { label: 'Again', hint: '<1m', cls: 'hover:border-red-400 hover:bg-red-500/5 hover:text-red-600' },
  { label: 'Hard', hint: '', cls: 'hover:border-amber-400 hover:bg-amber-500/5 hover:text-amber-600' },
  { label: 'Good', hint: '', cls: 'hover:border-emerald-400 hover:bg-emerald-500/5 hover:text-emerald-600' },
  { label: 'Easy', hint: '', cls: 'hover:border-sky-400 hover:bg-sky-500/5 hover:text-sky-600' },
];
</script>

<template>
  <div class="p-4">
    <Generating v-if="busy" label="Writing flashcards…" />

    <template v-else-if="reviewing && card">
      <div class="mb-3 flex items-center justify-between text-xs text-zinc-400">
        <span>{{ reviewed }} reviewed · {{ queue.length }} left</span>
        <button class="hover:text-zinc-700" @click="reviewing = false">End session</button>
      </div>
      <div class="h-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
        <div class="h-full rounded-full bg-accent-500 transition-all" :style="{ width: `${(reviewed / (reviewed + queue.length)) * 100}%` }" />
      </div>
      <div class="mt-4 [perspective:1200px]">
        <button class="relative h-64 w-full transition-transform duration-500 [transform-style:preserve-3d]" :class="flipped && '[transform:rotateY(180deg)]'" @click="flipped = !flipped">
          <div class="panel absolute inset-0 flex flex-col items-center justify-center p-6 text-center [backface-visibility:hidden]">
            <div class="label mb-3">Question</div>
            <div class="text-lg leading-snug font-medium">{{ card.front }}</div>
            <div class="absolute bottom-3 text-[11px] text-zinc-400">Click or press Space to flip</div>
          </div>
          <div class="panel absolute inset-0 flex flex-col items-center justify-center bg-gradient-to-br from-accent-500/5 to-fuchsia-500/5 p-6 text-center [backface-visibility:hidden] [transform:rotateY(180deg)]">
            <div class="label mb-3">Answer</div>
            <div class="text-[15px] leading-relaxed">{{ card.back }}</div>
            <div v-if="card.cite" class="prose-note mt-3" v-html="render(card.cite)" @click.stop />
          </div>
        </button>
      </div>
      <div class="mt-4 grid grid-cols-4 gap-2" :class="!flipped && 'pointer-events-none opacity-40'">
        <button v-for="(g, i) in GRADES" :key="g.label" class="btn-outline flex-col !gap-0 !py-2" :class="g.cls" @click="grade(i)">
          <span>{{ g.label }}</span><span class="text-[10px] text-zinc-400">{{ i + 1 }}</span>
        </button>
      </div>
    </template>

    <template v-else>
      <div v-if="reviewing" class="mb-4 flex flex-col items-center rounded-2xl bg-emerald-500/10 p-5 text-center">
        <PartyPopper class="size-6 text-emerald-500" />
        <div class="mt-2 font-semibold">Session complete</div>
        <div class="text-xs text-zinc-500">You reviewed {{ reviewed }} cards.</div>
      </div>
      <div class="grid grid-cols-3 gap-2 text-center">
        <div class="panel p-3"><div class="text-2xl font-bold">{{ deck.cards.length }}</div><div class="text-[11px] text-zinc-400">cards</div></div>
        <div class="panel p-3"><div class="text-2xl font-bold text-accent-600 dark:text-accent-400">{{ deck.due }}</div><div class="text-[11px] text-zinc-400">due now</div></div>
        <div class="panel p-3"><div class="text-2xl font-bold">{{ deck.cards.filter((c) => c.reps > 0).length }}</div><div class="text-[11px] text-zinc-400">learned</div></div>
      </div>
      <button class="btn-primary mt-4 w-full !py-2.5" :disabled="!deck.due" @click="start()"><Play class="size-4" /> Review {{ deck.due }} due</button>
      <button v-if="deck.cards.length" class="btn-ghost mt-1 w-full" @click="start(true)"><RotateCcw class="size-3.5" /> Cram all cards</button>

      <div class="panel mt-5 p-4">
        <div class="text-sm font-medium">Generate cards</div>
        <p class="mt-0.5 text-xs text-zinc-500">New cards get added to this deck. Your existing progress is kept.</p>
        <div class="mt-3 flex items-center gap-2">
          <select v-model.number="count" class="input !w-auto !py-1.5"><option :value="10">10 cards</option><option :value="15">15 cards</option><option :value="25">25 cards</option></select>
          <button class="btn-primary flex-1" @click="generate"><Sparkles class="size-3.5" /> Generate</button>
        </div>
      </div>
      <button v-if="deckPath" class="btn-ghost mt-2 w-full text-xs" @click="openTab(deckPath)"><ExternalLink class="size-3" /> Open deck in Obsidian (Spaced Repetition format)</button>
    </template>
  </div>
</template>
