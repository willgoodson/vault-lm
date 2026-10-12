<script setup>
// Floating AI actions for selected text in the editor.
import { ref, computed, watch } from 'vue';
import { Lightbulb, Wand2, Layers, ListChecks, MessageSquareQuote, Loader2, X, Check } from 'lucide-vue-next';
import { store, ns, selection, toast } from '../store.js';
import { api } from '../api.js';
import { render } from '../lib/markdown.js';

const props = defineProps({ sel: Object, path: String });
const emit = defineEmits(['replace', 'close']);
const busy = ref('');
const result = ref(null); // { action, text? card? question? }
const picked = ref(null);
watch(() => props.sel?.text, () => { if (!busy.value) { result.value = null; picked.value = null; } });

const scopeFor = () => ({ ...selection.value, topic: selection.value.topic || undefined });
async function run(action) {
  busy.value = action; result.value = null; picked.value = null;
  try { result.value = { action, ...(await api('/api/assist', { action, text: props.sel.text, ...scopeFor() })) }; }
  catch (e) { toast(e.message, 'error'); }
  busy.value = '';
}
async function addCard() {
  try {
    const sc = selection.value.topic ? selection.value : { notebook: store.notebook, topic: props.path };
    const r = await api('/api/study/card', { ...sc, card: result.value.card });
    toast(`Added to the deck (${r.cards.length} cards)`, 'success');
    emit('close');
  } catch (e) { toast(e.message, 'error'); }
}
function askInChat() {
  const quote = props.sel.text.split('\n').map((l) => `> ${l}`).join('\n');
  store.composer = `${quote}\n\n`;
  ns.value.active = 'chat';
  emit('close');
}
const style = computed(() => ({ top: `${Math.max(64, props.sel.top - 48)}px`, left: `${Math.min(props.sel.left, innerWidth - 460)}px` }));
const ACTIONS = [
  { id: 'explain', label: 'Explain', icon: Lightbulb },
  { id: 'simplify', label: 'Simplify', icon: Wand2 },
  { id: 'flashcard', label: 'Flashcard', icon: Layers },
  { id: 'quiz', label: 'Quiz me', icon: ListChecks },
];
</script>

<template>
  <Teleport to="body">
    <div class="fixed z-40" :style="style" @mousedown.prevent>
      <div class="panel flex items-center gap-0.5 p-1 shadow-xl">
        <button v-for="a in ACTIONS" :key="a.id" class="btn-ghost !px-2 !py-1 text-xs" :disabled="!!busy" @click="run(a.id)">
          <Loader2 v-if="busy === a.id" class="size-3.5 animate-spin" /><component :is="a.icon" v-else class="size-3.5" /> {{ a.label }}
        </button>
        <span class="mx-0.5 h-4 w-px bg-zinc-200 dark:bg-zinc-700" />
        <button class="btn-ghost !px-2 !py-1 text-xs" @click="askInChat"><MessageSquareQuote class="size-3.5" /> Ask in chat</button>
      </div>
      <div v-if="result" class="panel mt-1.5 w-[440px] p-3 shadow-xl" @mousedown.stop>
        <div class="mb-2 flex items-center">
          <span class="label">{{ ACTIONS.find((a) => a.id === result.action)?.label }}</span>
          <button class="btn-ghost ml-auto !p-1" @click="result = null"><X class="size-3.5" /></button>
        </div>
        <template v-if="result.text">
          <div class="prose-note max-h-72 overflow-y-auto text-sm" v-html="render(result.text)" />
          <button v-if="result.action === 'simplify'" class="btn-primary mt-3 w-full" @click="emit('replace', result.text); emit('close')"><Check class="size-3.5" /> Replace selection</button>
        </template>
        <template v-else-if="result.card">
          <label class="label">Front</label>
          <textarea v-model="result.card.front" rows="2" class="input mt-1 resize-none" />
          <label class="label mt-2 block">Back</label>
          <textarea v-model="result.card.back" rows="3" class="input mt-1 resize-none" />
          <button class="btn-primary mt-3 w-full" @click="addCard"><Layers class="size-3.5" /> Add to this topic's deck</button>
        </template>
        <template v-else-if="result.question">
          <div class="text-sm font-medium">{{ result.question.question }}</div>
          <button v-for="(c, i) in result.question.choices" :key="i" class="mt-1.5 flex w-full gap-2 rounded-lg border p-2 text-left text-sm transition"
            :class="picked === null ? 'border-zinc-200 hover:border-accent-400 dark:border-zinc-700' : i === result.question.answer ? 'border-emerald-500 bg-emerald-500/10' : i === picked ? 'border-red-500 bg-red-500/10' : 'border-zinc-200 opacity-50 dark:border-zinc-700'"
            @click="picked ??= i">
            <span class="font-semibold text-zinc-400">{{ 'ABCD'[i] }}</span>{{ c }}
          </button>
          <p v-if="picked !== null" class="mt-2 text-xs text-zinc-500">{{ result.question.explanation }}</p>
        </template>
      </div>
    </div>
  </Teleport>
</template>
