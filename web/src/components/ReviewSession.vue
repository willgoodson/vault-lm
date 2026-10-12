<script setup>
// Mixed flashcard review across decks.
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { PartyPopper } from 'lucide-vue-next';
import { api } from '../api.js';
import { render } from '../lib/markdown.js';

const props = defineProps({ cards: Array });
const emit = defineEmits(['done']);
const queue = ref([...props.cards]);
const flipped = ref(false);
const reviewed = ref(0);
const card = computed(() => queue.value[0]);
const deckName = (d) => d.split('/').map((s) => s.replace(/\.md$/, '')).slice(-2).join(' › ');

async function grade(g) {
  const c = card.value;
  if (!c || !flipped.value) return;
  await api('/api/study/review', { id: c.id, grade: g });
  queue.value.shift();
  if (g === 0) queue.value.push(c);
  reviewed.value++;
  flipped.value = false;
}
function onKey(e) {
  if (['INPUT', 'TEXTAREA'].includes(e.target.tagName)) return;
  if (e.code === 'Space') { e.preventDefault(); flipped.value = !flipped.value; }
  if (['1', '2', '3', '4'].includes(e.key)) grade(+e.key - 1);
}
onMounted(() => document.addEventListener('keydown', onKey));
onUnmounted(() => document.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="mx-auto max-w-xl">
    <template v-if="card">
      <div class="mb-3 flex items-center justify-between text-xs text-zinc-400">
        <span>{{ reviewed }} reviewed · {{ queue.length }} left</span>
        <button class="hover:text-zinc-700" @click="emit('done')">End session</button>
      </div>
      <div class="h-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800"><div class="h-full rounded-full bg-accent-500 transition-all" :style="{ width: `${(reviewed / (reviewed + queue.length)) * 100}%` }" /></div>
      <div class="mt-5 [perspective:1200px]">
        <button class="relative h-72 w-full transition-transform duration-500 [transform-style:preserve-3d]" :class="flipped && '[transform:rotateY(180deg)]'" @click="flipped = !flipped">
          <div class="panel absolute inset-0 flex flex-col items-center justify-center p-8 text-center [backface-visibility:hidden]">
            <div class="label mb-3">{{ deckName(card.deck) }}</div>
            <div class="text-xl leading-snug font-medium">{{ card.front }}</div>
            <div class="absolute bottom-3 text-[11px] text-zinc-400">Space to flip</div>
          </div>
          <div class="panel absolute inset-0 flex flex-col items-center justify-center bg-gradient-to-br from-accent-500/5 to-fuchsia-500/5 p-8 text-center [backface-visibility:hidden] [transform:rotateY(180deg)]">
            <div class="label mb-3">Answer</div>
            <div class="text-base leading-relaxed">{{ card.back }}</div>
            <div v-if="card.cite" class="prose-note mt-3" v-html="render(card.cite)" @click.stop />
          </div>
        </button>
      </div>
      <div class="mt-4 grid grid-cols-4 gap-2" :class="!flipped && 'pointer-events-none opacity-40'">
        <button v-for="(g, i) in ['Again', 'Hard', 'Good', 'Easy']" :key="g" class="btn-outline flex-col !gap-0 !py-2" @click="grade(i)">
          <span>{{ g }}</span><span class="text-[10px] text-zinc-400">{{ i + 1 }}</span>
        </button>
      </div>
    </template>
    <div v-else class="flex flex-col items-center rounded-2xl bg-emerald-500/10 p-8 text-center">
      <PartyPopper class="size-7 text-emerald-500" />
      <div class="mt-2 text-lg font-semibold">All caught up</div>
      <div class="text-sm text-zinc-500">You reviewed {{ reviewed }} card{{ reviewed === 1 ? '' : 's' }}.</div>
      <button class="btn-primary mt-4" @click="emit('done')">Back to dashboard</button>
    </div>
  </div>
</template>
