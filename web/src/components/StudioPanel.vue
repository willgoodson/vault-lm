<script setup>
import { ref, computed, defineAsyncComponent } from 'vue';
import { Layers, ListChecks, Network, Lightbulb, FileText, AudioLines, ArrowLeft, Maximize2, Minimize2, X } from 'lucide-vue-next';
import { store, ns, levels, scope, scopeName } from '../store.js';
import Flashcards from './studio/Flashcards.vue';
import Quiz from './studio/Quiz.vue';
import ExplainBack from './studio/ExplainBack.vue';
import Briefs from './studio/Briefs.vue';
import AudioOverview from './studio/AudioOverview.vue';

const MindMap = defineAsyncComponent(() => import('./studio/MindMap.vue'));
const TOOLS = [
  { id: 'flashcards', label: 'Flashcards', desc: 'Spaced repetition', icon: Layers, comp: Flashcards, tint: 'from-violet-500 to-fuchsia-500' },
  { id: 'quiz', label: 'Quiz', desc: 'Test yourself', icon: ListChecks, comp: Quiz, tint: 'from-sky-500 to-cyan-500' },
  { id: 'mindmap', label: 'Mind map', desc: 'See the structure', icon: Network, comp: MindMap, tint: 'from-emerald-500 to-teal-500', wide: true },
  { id: 'explain', label: 'Explain back', desc: 'Feynman technique', icon: Lightbulb, comp: ExplainBack, tint: 'from-amber-500 to-orange-500' },
  { id: 'briefs', label: 'Briefs', desc: 'Summary, FAQ, guide', icon: FileText, comp: Briefs, tint: 'from-rose-500 to-pink-500' },
  { id: 'audio', label: 'Audio overview', desc: 'Two-host podcast', icon: AudioLines, comp: AudioOverview, tint: 'from-indigo-500 to-blue-500' },
];
const tool = computed({ get: () => ns.value?.studio.tool ?? null, set: (v) => ns.value && (ns.value.studio.tool = v) });
const wide = ref(false);
const setLevel = (l) => (ns.value.studio.level = l);
const current = computed(() => TOOLS.find((t) => t.id === tool.value));
const level = computed(() => (levels.value.some((l) => l.level === ns.value?.studio.level) ? ns.value.studio.level : levels.value.at(-1)?.level));
const scopeKey = computed(() => JSON.stringify(scope(level.value)) + '|' + store.info.vault);
function pick(t) { tool.value = t.id; wide.value = !!t.wide; }
</script>

<template>
  <aside class="flex shrink-0 flex-col border-l border-zinc-200/80 bg-white/60 backdrop-blur transition-[width] duration-200 dark:border-zinc-800 dark:bg-zinc-900/40"
    :class="wide ? 'w-[min(720px,55vw)]' : 'w-[380px]'">
    <div class="flex h-14 shrink-0 items-center gap-2 border-b border-zinc-200/80 px-3 dark:border-zinc-800">
      <button v-if="current" class="btn-ghost !p-1.5" @click="tool = null; wide = false"><ArrowLeft class="size-4" /></button>
      <div class="font-semibold tracking-tight">{{ current?.label || 'Studio' }}</div>
      <div class="ml-auto flex items-center gap-1">
        <button v-if="current" class="btn-ghost !p-1.5" :title="wide ? 'Narrow' : 'Expand'" @click="wide = !wide"><Minimize2 v-if="wide" class="size-4" /><Maximize2 v-else class="size-4" /></button>
        <button class="btn-ghost !p-1.5" @click="store.studioOpen = false"><X class="size-4" /></button>
      </div>
    </div>

    <div class="border-b border-zinc-200/80 px-4 py-2.5 dark:border-zinc-800">
      <div class="label mb-1.5">Study</div>
      <div class="flex gap-1 rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800/80">
        <button v-for="l in levels" :key="l.level" class="flex min-w-0 flex-1 items-center justify-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition" @click="setLevel(l.level)"
          :class="level === l.level ? 'bg-white text-zinc-900 shadow-sm dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'" :title="`${l.level}: ${l.name}`">
          <span class="shrink-0 opacity-70">{{ l.icon }}</span><span class="truncate">{{ l.name }}</span>
        </button>
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto">
      <div v-if="!current" class="grid grid-cols-2 gap-2.5 p-4">
        <button v-for="t in TOOLS" :key="t.id" :disabled="!store.notebook" @click="pick(t)"
          class="panel group flex flex-col items-start gap-3 p-3.5 text-left transition hover:-translate-y-0.5 hover:shadow-md disabled:opacity-50">
          <div class="grid size-9 place-items-center rounded-xl bg-gradient-to-br text-white shadow-sm" :class="t.tint"><component :is="t.icon" class="size-4.5" /></div>
          <div>
            <div class="text-sm font-semibold">{{ t.label }}</div>
            <div class="text-xs text-zinc-500">{{ t.desc }}</div>
          </div>
        </button>
        <p class="col-span-2 mt-2 text-center text-[11px] text-zinc-400">Studying <b>{{ scopeName(scope(level)) }}</b>. Outputs are saved to <code>_studio/</code>, and they're never indexed.</p>
      </div>
      <KeepAlive v-else :max="6">
        <component :is="current.comp" :key="current.id + scopeKey" />
      </KeepAlive>
    </div>
  </aside>
</template>
