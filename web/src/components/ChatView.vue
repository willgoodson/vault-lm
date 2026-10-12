<script setup>
import { ref, computed, nextTick, watch } from 'vue';
import { ArrowUp, Sparkles, ChevronDown, Square, Layers, BookOpen, FilePlus2 } from 'lucide-vue-next';
import ChatSwitcher from './ChatSwitcher.vue';
import SaveAnswerPopover from './SaveAnswerPopover.vue';
import { api } from '../api.js';
import { store, ns, selection, breadcrumb, levels, openTab, currentNb, currentSection, currentChat } from '../store.js';
import { Target } from 'lucide-vue-next';
import ModeSelect from './ModeSelect.vue';
import { stream } from '../api.js';
import { render } from '../lib/markdown.js';

const log = ref(null);
const input = ref(null);
const busy = ref(false);
const thread = computed(() => currentChat.value?.messages ?? []);
const mode = computed(() => currentChat.value?.mode || 'reference');
const save = ref({ open: false, anchor: null, msg: null });
const openSave = (e, m) => (save.value = { open: true, anchor: e.currentTarget, msg: m });
const scopeLabel = computed(() => levels.value.at(-1)?.name || '');

const detailsFor = computed(() => [currentSection.value, currentNb.value].filter((x) => x?.meta && (x.meta.description || x.meta.goals?.length)));
const SUGGEST = ['Summarize the key ideas', 'What are the most important open questions?', 'Explain the main concept like I’m new to it'];

const scroll = () => nextTick(() => log.value?.scrollTo({ top: log.value.scrollHeight, behavior: 'smooth' }));
watch(() => [store.notebook, currentChat.value?.id], scroll);

async function ask(q = store.composer) {
  q = q.trim();
  if (!q || busy.value || !store.notebook) return;
  const sel = { ...selection.value };
  store.composer = '';
  busy.value = true;
  const history = thread.value.flatMap((m) => [{ role: 'user', content: m.q }, { role: 'assistant', content: m.a }]);
  thread.value.push({ q, a: '', sources: [], followups: [], mode: mode.value, scope: breadcrumb.value, open: false });
  const msg = thread.value.at(-1);
  scroll();
  try {
    await stream('/api/chat', { question: q, ...sel, mode: mode.value, history }, (e) => {
      if (e.token) { msg.a += e.token; scroll(); }
      if (e.sources) msg.sources = e.sources;
      if (e.followups) { msg.followups = e.followups; scroll(); }
      if (e.error) msg.a += `\n\n**Error:** ${e.error}`;
    });
  } catch (e) { msg.a += `\n\n**Error:** ${e.message}`; }
  busy.value = false;
  nextTick(() => input.value?.focus());
  // Name the chat after its first exchange.
  const chat = currentChat.value;
  if (chat && !chat.titled && chat.messages.length === 1 && msg.a) {
    chat.titled = true;
    api('/api/chat/title', { question: q, answer: msg.a }).then((r) => { if (r.title) chat.title = r.title; }).catch(() => {});
  }
}

function onKey(e) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); ask(); }
}
const kindStyle = { primary: 'border-accent-500', notebook: 'border-ref-500', overview: 'border-zinc-400' };
</script>

<template>
  <div class="flex h-full flex-col">
    <div v-if="ns" class="flex items-center border-b border-zinc-100 px-4 py-1.5 dark:border-zinc-800/60"><ChatSwitcher /></div>
    <div ref="log" class="min-h-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-3xl px-6 py-8">
        <div v-if="!thread.length" class="flex flex-col items-center pt-16 text-center">
          <div class="grid size-14 place-items-center rounded-2xl bg-gradient-to-br from-accent-500 to-fuchsia-500 text-white shadow-lg shadow-accent-500/30">
            <Sparkles class="size-6" />
          </div>
          <h1 class="mt-5 text-2xl font-semibold tracking-tight">{{ scopeLabel || 'Pick a notebook' }}</h1>
          <p v-if="levels.length > 1" class="mt-0.5 text-xs text-zinc-400">{{ breadcrumb }}</p>
          <p class="mt-1.5 text-sm text-zinc-500">Ask anything. Answers cite your notes, and clicking a citation opens it.</p>
          <div v-for="d in detailsFor" :key="d.path" class="panel mt-5 w-full max-w-xl p-4 text-left">
            <div class="label">{{ d.name }}</div>
            <p v-if="d.meta.description" class="mt-1 text-sm text-zinc-600 dark:text-zinc-300">{{ d.meta.description }}</p>
            <div v-if="d.meta.goals?.length" class="mt-2.5 flex flex-wrap gap-1.5">
              <button v-for="g in d.meta.goals" :key="g" class="chip bg-accent-500/10 py-1 text-accent-700 hover:bg-accent-500/20 dark:text-accent-300" title="Quiz me on this" @click="store.composer = `Quiz me on: ${g}`">
                <Target class="size-3" /> {{ g }}
              </button>
            </div>
          </div>
          <div class="mt-6 flex flex-wrap justify-center gap-2">
            <button v-for="s in SUGGEST" :key="s" class="btn-outline rounded-full text-zinc-600 dark:text-zinc-300" @click="ask(s)">{{ s }}</button>
          </div>
        </div>

        <div v-for="(m, i) in thread" :key="i" class="mb-8">
          <div class="flex justify-end">
            <div class="max-w-[85%] rounded-2xl rounded-br-md bg-accent-600 px-4 py-2.5 text-[15px] text-white shadow-sm">{{ m.q }}</div>
          </div>
          <div class="mt-1 text-right text-[11px] text-zinc-400">{{ m.mode }} · {{ m.scope }}</div>

          <div class="mt-3 flex gap-3">
            <div class="grid size-7 shrink-0 place-items-center rounded-lg bg-gradient-to-br from-accent-500 to-fuchsia-500 text-white"><Sparkles class="size-3.5" /></div>
            <div class="min-w-0 flex-1">
              <div v-if="!m.a" class="space-y-2 pt-1"><div class="skeleton h-3 w-3/4" /><div class="skeleton h-3 w-1/2" /></div>
              <div class="prose-note" v-html="render(m.a, m.sources)" />

              <div v-if="m.a && !(busy && i === thread.length - 1)" class="mt-2 flex">
                <button class="btn-ghost -ml-2 !px-2 !py-1 text-xs" title="Save this answer as a topic" @click="openSave($event, m)"><FilePlus2 class="size-3.5" /> Save as topic</button>
              </div>
              <div v-if="m.sources.length" class="mt-1">
                <button class="flex items-center gap-1 text-xs font-medium text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200" @click="m.open = !m.open">
                  <ChevronDown class="size-3.5 transition" :class="m.open && 'rotate-180'" />
                  {{ m.sources.length }} sources
                  <span v-if="m.sources.some((s) => s.kind === 'notebook')" class="chip ml-1 bg-ref-500/15 text-amber-700 dark:text-amber-300"><Layers class="size-2.5" />from the notebook</span>
                  <span v-if="m.sources.some((s) => s.kind === 'overview')" class="chip bg-zinc-100 text-zinc-500 dark:bg-zinc-800">overview</span>
                </button>
                <div v-if="m.open" class="mt-2 grid gap-2 sm:grid-cols-2">
                  <button v-for="(s, j) in m.sources" :key="j" class="panel border-l-[3px] p-3 text-left transition hover:shadow-md" :class="kindStyle[s.kind]"
                    @click="openTab(s.path, s.heading)">
                    <div class="flex items-center gap-1.5 text-xs font-medium">
                      <span class="truncate">{{ s.note }}<span v-if="s.heading" class="text-zinc-400"> › {{ s.heading }}</span></span>
                      <span v-if="s.kind !== 'primary'" class="chip ml-auto shrink-0 bg-zinc-100 text-zinc-500 dark:bg-zinc-800">{{ s.kind === 'notebook' ? s.origin : s.kind }}</span>
                    </div>
                    <p class="mt-1 line-clamp-3 text-xs text-zinc-500">{{ s.text.replace(/^#+.*\n/, '') }}</p>
                  </button>
                </div>
              </div>

              <div v-if="m.followups.length && i === thread.length - 1" class="mt-4 flex flex-wrap gap-2">
                <button v-for="f in m.followups" :key="f" class="rounded-full border border-zinc-200 px-3 py-1 text-left text-xs text-zinc-600 transition hover:border-accent-400 hover:text-accent-600 dark:border-zinc-700 dark:text-zinc-400" @click="ask(f)">{{ f }}</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="mx-auto w-full max-w-3xl px-6 pb-5">
      <div v-if="store.notebook" class="mb-2 flex items-center gap-2">
        <ModeSelect />
        <div class="flex min-w-0 items-center gap-1 truncate text-xs text-zinc-500" title="Change the scope in the Notebooks list">
          <BookOpen class="size-3.5 shrink-0" /><span class="truncate">{{ breadcrumb }}</span>
        </div>
      </div>
      <div class="panel flex items-end gap-2 p-2 shadow-lg shadow-zinc-900/5 focus-within:border-accent-400 focus-within:ring-4 focus-within:ring-accent-500/10">
        <textarea ref="input" v-model="store.composer" rows="1" @keydown="onKey" :disabled="!store.notebook"
          class="max-h-40 min-h-9 flex-1 resize-none bg-transparent px-2 py-1.5 text-[15px] outline-none placeholder:text-zinc-400"
          :placeholder="store.notebook ? `Ask ${scopeLabel}…` : 'Create a notebook to start'"
          @input="(e) => { e.target.style.height = 'auto'; e.target.style.height = e.target.scrollHeight + 'px'; }" />
        <button class="btn-primary size-9 !rounded-xl !p-0" :disabled="busy || !store.composer.trim()" @click="ask()">
          <Square v-if="busy" class="size-3.5 animate-pulse" /><ArrowUp v-else class="size-4" />
        </button>
      </div>
      <p class="mt-2 text-center text-[11px] text-zinc-400">
        <button class="rounded px-1 font-medium hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-200" title="Change model" @click="store.modelsOpen = true">
          {{ store.info.cloud ? '☁' : '⌂' }} {{ store.info.model }} ▾</button>
        · {{ store.info.cloud ? 'Cloud model. Retrieved excerpts are sent to the provider.' : 'Runs locally.' }} Click a citation to open it.
      </p>
    </div>
    <SaveAnswerPopover v-if="save.msg" :anchor="save.anchor" :open="save.open" :msg="save.msg" @close="save.open = false" />
  </div>
</template>
