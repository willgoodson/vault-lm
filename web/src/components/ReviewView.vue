<script setup>
import { ref, computed, onMounted, onActivated, watch } from 'vue';
import { Flame, Layers, Target, CalendarClock, Play, Crosshair, Loader2, TrendingUp, Trophy } from 'lucide-vue-next';
import { store, openTab, resolveNote, selectNotebook, toast } from '../store.js';
import { api } from '../api.js';
import ReviewSession from './ReviewSession.vue';
import Quiz from './studio/Quiz.vue';

const nb = ref('');
const data = ref(null);
const session = ref(null);   // { kind: 'cards', cards } | { kind: 'drill', questions, notebook }
const drilling = ref(false);

async function load() {
  data.value = await api(`/api/progress/summary${nb.value ? `?notebook=${encodeURIComponent(nb.value)}` : ''}`).catch(() => null);
  if (data.value && !nb.value) store.dueTotal = data.value.dueTotal;
}
onMounted(load);
onActivated(load);
watch(nb, load);

async function startCards() {
  const cards = await api(`/api/progress/due${nb.value ? `?notebook=${encodeURIComponent(nb.value)}` : ''}`);
  if (!cards.length) return toast('No cards are due', 'info');
  session.value = { kind: 'cards', cards: cards.sort(() => Math.random() - 0.5) };
}
async function drill(spots) {
  const notebook = nb.value || spots[0]?.notebook;
  drilling.value = true;
  try {
    const r = await api('/api/study/drill', { notebook, concepts: spots.map((s) => s.concept), count: Math.min(5, spots.length + 2) });
    session.value = { kind: 'drill', questions: r.questions, notebook };
  } catch (e) { toast(e.message, 'error'); }
  drilling.value = false;
}
function endSession() { session.value = null; load(); }
function openConcept(s) {
  const p = resolveNote(s.note);
  if (!p) return;
  const nbPath = store.notebooks.find((n) => p.startsWith(n.path + '/'))?.path;
  if (nbPath) selectNotebook(nbPath);
  openTab(p, s.heading || undefined);
}

// Heatmap: 12 weeks × 7 days.
const weeks = computed(() => {
  const h = data.value?.heat || [];
  const out = [];
  for (let i = 0; i < h.length; i += 7) out.push(h.slice(i, i + 7));
  return out;
});
const heatColor = (n) => (n === 0 ? 'fill-zinc-100 dark:fill-zinc-800' : n < 3 ? 'fill-accent-200 dark:fill-accent-800' : n < 8 ? 'fill-accent-400' : 'fill-accent-600');
const spark = computed(() => {
  const a = data.value?.accuracy || [];
  if (a.length < 2) return '';
  return a.map((p, i) => `${(i / (a.length - 1)) * 200},${40 - p.pct * 36 - 2}`).join(' ');
});
const avgAcc = computed(() => { const a = data.value?.accuracy || []; return a.length ? Math.round((a.reduce((s, p) => s + p.pct, 0) / a.length) * 100) : null; });
const ago = (t) => { const d = Math.floor((Date.now() - t) / 86400000); return d <= 0 ? 'today' : d === 1 ? 'yesterday' : `${d}d ago`; };
const ring = (d) => (d.total ? 1 - d.due / d.total : 1);
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="mx-auto max-w-5xl px-8 py-8">
      <div class="flex items-center gap-3">
        <div>
          <h1 class="text-2xl font-semibold tracking-tight">Review</h1>
          <p class="text-sm text-zinc-500">Cards due, weak spots and progress{{ nb ? '' : ' across every notebook' }}.</p>
        </div>
        <select v-model="nb" class="input ml-auto !w-56 !py-1.5">
          <option value="">All notebooks</option>
          <option v-for="n in store.notebooks" :key="n.path" :value="n.path">{{ n.name }}</option>
        </select>
      </div>

      <div v-if="session" class="mt-8">
        <ReviewSession v-if="session.kind === 'cards'" :cards="session.cards" @done="endSession" />
        <div v-else class="mx-auto max-w-xl">
          <div class="label mb-3 flex items-center gap-1.5"><Crosshair class="size-3" /> Weak-spot drill</div>
          <div class="panel"><Quiz :initial="session.questions" :notebook="session.notebook" @done="endSession" /></div>
        </div>
      </div>

      <template v-else-if="data">
        <!-- stat tiles -->
        <div class="mt-6 grid grid-cols-4 gap-3">
          <div class="panel p-4"><div class="label flex items-center gap-1.5"><Layers class="size-3" /> Due now</div><div class="mt-1 text-3xl font-bold text-accent-600 dark:text-accent-400">{{ data.dueTotal }}</div><div class="text-xs text-zinc-400">of {{ data.totalCards }} cards</div></div>
          <div class="panel p-4"><div class="label flex items-center gap-1.5"><Trophy class="size-3" /> Learned</div><div class="mt-1 text-3xl font-bold">{{ data.learned }}</div><div class="text-xs text-zinc-400">cards reviewed at least once</div></div>
          <div class="panel p-4"><div class="label flex items-center gap-1.5"><Flame class="size-3" /> Streak</div><div class="mt-1 text-3xl font-bold">{{ data.streak }}<span class="text-base font-medium text-zinc-400"> day{{ data.streak === 1 ? '' : 's' }}</span></div><div class="text-xs text-zinc-400">studied in a row</div></div>
          <div class="panel p-4"><div class="label flex items-center gap-1.5"><TrendingUp class="size-3" /> Quiz accuracy</div><div class="mt-1 text-3xl font-bold">{{ avgAcc ?? '—' }}<span v-if="avgAcc != null" class="text-base font-medium text-zinc-400">%</span></div><div class="text-xs text-zinc-400">{{ data.quizAnswered }} answers, last 30 days</div></div>
        </div>

        <div class="mt-4 grid grid-cols-5 gap-4">
          <!-- Today -->
          <section class="panel col-span-3 p-5">
            <div class="flex items-center">
              <h2 class="font-semibold">Today</h2>
              <button class="btn-primary ml-auto" :disabled="!data.dueTotal" @click="startCards"><Play class="size-3.5" /> Review {{ data.dueTotal }} due</button>
            </div>
            <p v-if="!data.decks.length" class="mt-4 text-sm text-zinc-400">No flashcards yet. Make some in the Studio or from selected text.</p>
            <div class="mt-4 grid grid-cols-3 gap-3">
              <div v-for="d in data.decks" :key="d.notebook" class="flex items-center gap-3 rounded-xl border border-zinc-100 p-3 dark:border-zinc-800">
                <svg viewBox="0 0 36 36" class="size-10 shrink-0 -rotate-90">
                  <circle cx="18" cy="18" r="15" fill="none" stroke-width="4" class="stroke-zinc-100 dark:stroke-zinc-800" />
                  <circle cx="18" cy="18" r="15" fill="none" stroke-width="4" stroke-linecap="round" class="stroke-accent-500" :stroke-dasharray="94.2" :stroke-dashoffset="94.2 * (1 - ring(d))" />
                </svg>
                <div class="min-w-0"><div class="truncate text-sm font-medium">{{ d.name }}</div><div class="text-xs text-zinc-400">{{ d.due }} due · {{ d.total }} cards</div></div>
              </div>
            </div>
          </section>

          <!-- Upcoming -->
          <section class="panel col-span-2 p-5">
            <h2 class="flex items-center gap-1.5 font-semibold"><CalendarClock class="size-4" /> Upcoming</h2>
            <p v-if="!data.upcoming.length" class="mt-3 text-sm text-zinc-400">Give a notebook or section a due date in <i>Edit details…</i> to see it here.</p>
            <div v-for="u in data.upcoming" :key="u.path" class="mt-3 flex items-center gap-3">
              <div class="w-14 shrink-0 text-center"><div class="text-lg leading-none font-bold" :class="u.daysLeft < 7 ? 'text-red-500' : ''">{{ Math.max(0, u.daysLeft) }}</div><div class="text-[10px] text-zinc-400">days</div></div>
              <div class="min-w-0 flex-1"><div class="truncate text-sm font-medium">{{ u.name }}</div><div class="text-xs text-zinc-400">due {{ u.due }} · {{ u.cardsDueBefore }}/{{ u.total }} cards due before then</div></div>
            </div>
          </section>

          <!-- Weak spots -->
          <section class="panel col-span-3 p-5">
            <div class="flex items-center">
              <h2 class="flex items-center gap-1.5 font-semibold"><Target class="size-4" /> Weak spots</h2>
              <button class="btn-primary ml-auto" :disabled="!data.weak.length || drilling" @click="drill(data.weak.slice(0, 4))">
                <Loader2 v-if="drilling" class="size-3.5 animate-spin" /><Crosshair v-else class="size-3.5" /> Drill weak spots
              </button>
            </div>
            <p v-if="!data.weak.length" class="mt-3 text-sm text-zinc-400">Nothing yet. Missed quiz questions, “Again” cards and explain-back gaps show up here.</p>
            <div v-for="s in data.weak" :key="s.concept" class="mt-3 flex items-center gap-3">
              <div class="w-12 shrink-0 text-right text-sm font-semibold" :class="s.accuracy < 0.4 ? 'text-red-500' : 'text-amber-500'">{{ Math.round(s.accuracy * 100) }}%</div>
              <div class="min-w-0 flex-1">
                <button class="truncate text-left text-sm font-medium hover:text-accent-600" @click="openConcept(s)">{{ s.note }}<span v-if="s.heading" class="text-zinc-400"> › {{ s.heading }}</span></button>
                <div class="truncate text-xs text-zinc-400">{{ s.misses }} missed of {{ s.attempts }} · last {{ ago(s.last) }}<span v-if="s.labels.length"> · “{{ s.labels[0] }}”</span></div>
              </div>
              <button class="btn-ghost !px-2 !py-1 text-xs" @click="drill([s])">Drill</button>
            </div>
          </section>

          <!-- Activity -->
          <section class="panel col-span-2 p-5">
            <h2 class="font-semibold">Activity</h2>
            <svg :viewBox="`0 0 ${weeks.length * 14} ${7 * 14}`" class="mt-3 w-full">
              <g v-for="(w, x) in weeks" :key="x">
                <rect v-for="(d, y) in w" :key="d.date" :x="x * 14" :y="y * 14" width="11" height="11" rx="2.5" :class="heatColor(d.count)"><title>{{ d.date }}: {{ d.count }} answers</title></rect>
              </g>
            </svg>
            <div class="mt-4 label">Quiz accuracy</div>
            <svg v-if="spark" viewBox="0 0 200 40" class="mt-1 h-10 w-full overflow-visible"><polyline :points="spark" fill="none" stroke-width="2" class="stroke-accent-500" stroke-linejoin="round" stroke-linecap="round" /></svg>
            <p v-else class="mt-1 text-xs text-zinc-400">Take a few quizzes to see a trend.</p>
          </section>
        </div>
      </template>
      <div v-else class="mt-8 space-y-3"><div class="skeleton h-24" /><div class="skeleton h-48" /></div>
    </div>
  </div>
</template>
