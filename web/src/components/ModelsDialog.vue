<script setup>
import { ref, computed, watch } from 'vue';
import { X, Cpu, Cloud, Server, Check, Loader2, RefreshCw, Download, KeyRound, ShieldAlert, Sparkles, Database } from 'lucide-vue-next';
import { store, toast } from '../store.js';
import { api } from '../api.js';

const props = defineProps({ open: Boolean });
const emit = defineEmits(['close']);

const cfg = ref(null);
const tab = ref('chat');            // 'chat' | 'embed'
const provider = ref('ollama');     // provider being browsed
const list = ref({});               // provider -> models[] | { error }
const loading = ref('');
const keyInput = ref({ anthropic: '', openai: '' });
const urlInput = ref({ ollama: '', openai: '' });
const label = ref('');
const pullName = ref('');

const PRESETS = [
  { label: 'OpenAI', url: 'https://api.openai.com/v1' },
  { label: 'OpenRouter', url: 'https://openrouter.ai/api/v1' },
  { label: 'Groq', url: 'https://api.groq.com/openai/v1' },
  { label: 'LM Studio', url: 'http://localhost:1234/v1' },
  { label: 'llama.cpp', url: 'http://localhost:8080/v1' },
];
const PROVIDERS = computed(() => [
  { id: 'ollama', label: 'Local · Ollama', icon: Cpu },
  ...(tab.value === 'chat' ? [{ id: 'anthropic', label: 'Claude · Anthropic', icon: Sparkles }] : []),
  { id: 'openai', label: cfg.value?.openai.label || 'OpenAI-compatible', icon: Cloud },
]);

async function load() {
  cfg.value = await api('/api/models');
  urlInput.value = { ollama: cfg.value.ollama.baseUrl, openai: cfg.value.openai.baseUrl };
  label.value = cfg.value.openai.label;
  provider.value = cfg.value[tab.value].provider;
  refresh(provider.value);
}
watch(() => props.open, (o) => o && load());
watch(tab, (t) => { if (cfg.value) { provider.value = t === 'embed' && provider.value === 'anthropic' ? 'ollama' : cfg.value[t].provider; refresh(provider.value); } });
watch(provider, (p) => !list.value[p] && refresh(p));

async function refresh(p) {
  loading.value = p;
  try { list.value[p] = await api(`/api/models/list?provider=${p}`); }
  catch (e) { list.value[p] = { error: e.message }; }
  loading.value = '';
}

const models = computed(() => {
  const l = list.value[provider.value];
  if (!Array.isArray(l)) return [];
  const want = tab.value === 'embed' ? 'embed' : 'chat';
  // Cloud lists can be huge and mixed; show the matching kind first but keep everything selectable.
  // Local lists are reliably labeled: only offer embedding models for embeddings.
  const rows = provider.value === 'ollama' && want === 'embed' ? l.filter((m) => m.kind === 'embed') : l;
  return [...rows].sort((a, b) => (b.kind === want) - (a.kind === want));
});
const norm = (m) => String(m || '').replace(/:latest$/, '');
const isCurrent = (m) => cfg.value?.[tab.value].provider === provider.value && norm(cfg.value[tab.value].model) === norm(m.id);
const gb = (b) => (b ? `${(b / 1e9).toFixed(b > 1e10 ? 0 : 1)} GB` : '');
const recommended = (m) => provider.value === 'anthropic' && m.id === 'claude-opus-5-5';

async function saveConn(p) {
  const patch = p === 'ollama' ? { baseUrl: urlInput.value.ollama }
    : p === 'anthropic' ? { apiKey: keyInput.value.anthropic || null }
    : { baseUrl: urlInput.value.openai, label: label.value || 'OpenAI-compatible', apiKey: keyInput.value.openai || null };
  loading.value = p;
  try {
    const r = await api('/api/models/test', { provider: p, settings: patch });
    toast(`Connected · ${r.models} models · ${r.ms} ms`, 'success');
    keyInput.value[p] = '';
    cfg.value = await api('/api/models');
    await refresh(p);
  } catch (e) { toast(e.message, 'error'); list.value[p] = { error: e.message }; }
  loading.value = '';
}

async function use(m) {
  const r = await api('/api/models', { [tab.value]: { provider: provider.value, model: m.id } }, 'PUT');
  cfg.value = r;
  store.info = await api('/api/info');
  toast(tab.value === 'chat' ? `Chat now uses ${m.name}` : `Embeddings now use ${m.name}${r.reindexed ? '. Rebuilding the search index…' : ''}`, 'success');
}
async function setEffort(e) { cfg.value = await api('/api/models', { anthropic: { effort: e } }, 'PUT'); }
async function pull() {
  const name = pullName.value.trim();
  if (!name) return;
  try { await api('/api/models/pull', { name }); toast(`Downloading ${name}… it will appear here when done`, 'info'); pullName.value = ''; }
  catch (e) { toast(e.message, 'error'); }
}
// Refresh the local list when a download finishes.
watch(() => Object.keys(store.jobs).length, (n, o) => { if (n < o && provider.value === 'ollama') refresh('ollama'); });

const cloudNote = computed(() => {
  const p = provider.value;
  if (p === 'anthropic') return 'Your question and the note excerpts retrieved for it are sent to Anthropic.';
  if (p === 'openai' && !/localhost|127\.0\.0\.1/.test(urlInput.value.openai)) return `Your question and the retrieved note excerpts are sent to ${new URL(urlInput.value.openai || 'https://x').host}.`;
  return '';
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fixed inset-0 z-50 grid place-items-center bg-zinc-900/30 backdrop-blur-sm" @mousedown.self="emit('close')">
      <div class="panel flex h-[620px] w-[860px] flex-col overflow-hidden shadow-2xl">
        <div class="flex items-center gap-3 border-b border-zinc-100 px-5 py-3.5 dark:border-zinc-800">
          <div class="font-semibold">Models</div>
          <div class="flex rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800/80">
            <button v-for="t in [{ id: 'chat', label: 'Chat & study' }, { id: 'embed', label: 'Search embeddings' }]" :key="t.id" class="rounded-md px-3 py-1 text-xs font-medium"
              :class="tab === t.id ? 'bg-white shadow-sm dark:bg-zinc-700' : 'text-zinc-500'" @click="tab = t.id">{{ t.label }}</button>
          </div>
          <div v-if="cfg" class="ml-2 truncate text-xs text-zinc-400">Current: <b class="text-zinc-700 dark:text-zinc-200">{{ cfg[tab].model }}</b> · {{ cfg[tab].provider }}</div>
          <button class="btn-ghost ml-auto !p-1" @click="emit('close')"><X class="size-4" /></button>
        </div>

        <div v-if="cfg" class="flex min-h-0 flex-1">
          <!-- providers -->
          <div class="w-52 shrink-0 space-y-1 border-r border-zinc-100 p-2 dark:border-zinc-800">
            <button v-for="p in PROVIDERS" :key="p.id" class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm"
              :class="provider === p.id ? 'bg-accent-500/10 text-accent-700 dark:text-accent-300' : 'hover:bg-zinc-100 dark:hover:bg-zinc-800'" @click="provider = p.id">
              <component :is="p.icon" class="size-4 shrink-0" /><span class="truncate">{{ p.label }}</span>
              <Check v-if="cfg[tab].provider === p.id" class="ml-auto size-3.5 text-accent-500" />
            </button>
            <p v-if="tab === 'embed'" class="px-2.5 pt-3 text-[11px] leading-snug text-zinc-400">Anthropic doesn't offer embeddings. Switching the embedding model builds a separate search index, and the old one is kept, so you can switch back instantly.</p>
          </div>

          <!-- detail -->
          <div class="flex min-w-0 flex-1 flex-col">
            <div class="space-y-2.5 border-b border-zinc-100 p-4 dark:border-zinc-800">
              <template v-if="provider === 'ollama'">
                <label class="label">Ollama server</label>
                <div class="flex gap-2"><input v-model="urlInput.ollama" class="input !py-1.5" /><button class="btn-outline" :disabled="!!loading" @click="saveConn('ollama')">Connect</button></div>
              </template>
              <template v-else-if="provider === 'anthropic'">
                <label class="label flex items-center gap-1"><KeyRound class="size-3" /> API key</label>
                <div class="flex gap-2">
                  <input v-model="keyInput.anthropic" type="password" autocomplete="off" class="input !py-1.5" :placeholder="cfg.anthropic.hasKey ? `Saved: ${cfg.anthropic.keyHint}${cfg.anthropic.fromEnv ? ' (from ANTHROPIC_API_KEY)' : ''}` : 'sk-ant-…'" />
                  <button class="btn-primary" :disabled="!!loading || (!keyInput.anthropic && !cfg.anthropic.hasKey)" @click="saveConn('anthropic')">{{ keyInput.anthropic ? 'Save & test' : 'Test' }}</button>
                </div>
                <div class="flex items-center gap-2 text-xs text-zinc-500">
                  <span>Effort</span>
                  <div class="flex rounded-md bg-zinc-100 p-0.5 dark:bg-zinc-800">
                    <button v-for="e in ['low', 'medium', 'high']" :key="e" class="rounded px-2 py-0.5 capitalize" :class="cfg.anthropic.effort === e ? 'bg-white shadow-sm dark:bg-zinc-700' : ''" @click="setEffort(e)">{{ e }}</button>
                  </div>
                  <span class="text-zinc-400">Higher effort gives more thorough answers, but they're slower and cost more.</span>
                </div>
              </template>
              <template v-else>
                <div class="flex flex-wrap gap-1.5">
                  <button v-for="pr in PRESETS" :key="pr.label" class="chip bg-zinc-100 py-1 text-zinc-600 hover:bg-accent-500/10 dark:bg-zinc-800 dark:text-zinc-300" @click="urlInput.openai = pr.url; label = pr.label">{{ pr.label }}</button>
                </div>
                <div class="grid grid-cols-[1fr_2fr] gap-2">
                  <input v-model="label" class="input !py-1.5" placeholder="Name" />
                  <input v-model="urlInput.openai" class="input !py-1.5" placeholder="https://…/v1" />
                </div>
                <div class="flex gap-2">
                  <input v-model="keyInput.openai" type="password" autocomplete="off" class="input !py-1.5" :placeholder="cfg.openai.hasKey ? `Saved: ${cfg.openai.keyHint}` : 'API key (leave empty for local servers)'" />
                  <button class="btn-primary" :disabled="!!loading" @click="saveConn('openai')">Save & test</button>
                </div>
              </template>
              <div v-if="cloudNote" class="flex items-start gap-2 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-800 dark:text-amber-200"><ShieldAlert class="mt-px size-3.5 shrink-0" /> {{ cloudNote }} Your vault files stay on this computer.</div>
            </div>

            <div class="flex items-center px-4 pt-3 pb-1">
              <span class="label">{{ tab === 'embed' ? 'Embedding models' : 'Models' }}</span>
              <button class="btn-ghost ml-auto !p-1" title="Refresh" @click="refresh(provider)"><RefreshCw class="size-3.5" :class="loading === provider && 'animate-spin'" /></button>
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
              <div v-if="list[provider]?.error" class="m-2 rounded-lg px-3 py-2 text-sm" :class="/No .*key yet/.test(list[provider].error) ? 'bg-zinc-100 text-zinc-500 dark:bg-zinc-800' : 'bg-red-500/10 text-red-600 dark:text-red-400'">{{ /No Anthropic API key/.test(list[provider].error) ? 'Add your Anthropic API key above to see the available Claude models.' : list[provider].error }}</div>
              <div v-else-if="loading === provider && !models.length" class="space-y-2 p-2"><div class="skeleton h-9" /><div class="skeleton h-9" /></div>
              <p v-else-if="!models.length" class="p-3 text-sm text-zinc-400">{{ tab === 'embed' && provider === 'ollama' ? 'No embedding models installed. Download one below (e.g. nomic-embed-text, mxbai-embed-large).' : 'No models found.' }}</p>
              <button v-for="m in models" :key="m.id" class="group flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition"
                :class="isCurrent(m) ? 'bg-accent-500/10' : 'hover:bg-zinc-100 dark:hover:bg-zinc-800'" @click="!isCurrent(m) && use(m)">
                <component :is="m.kind === 'embed' ? Database : provider === 'ollama' ? Cpu : Server" class="size-4 shrink-0 text-zinc-400" />
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-1.5 truncate text-sm">{{ m.name }}
                    <span v-if="recommended(m)" class="chip bg-accent-500/15 text-accent-700 dark:text-accent-300">recommended</span>
                    <span v-if="m.kind !== (tab === 'embed' ? 'embed' : 'chat')" class="chip bg-zinc-100 text-zinc-400 dark:bg-zinc-800">{{ m.kind }}</span>
                  </div>
                  <div class="truncate text-[11px] text-zinc-400">{{ [m.id !== m.name && m.id, m.params, m.quant, gb(m.size), m.context && `${Math.round(m.context / 1000)}K context`].filter(Boolean).join(' · ') }}</div>
                </div>
                <span v-if="isCurrent(m)" class="chip bg-accent-500 text-white"><Check class="size-3" /> in use</span>
                <span v-else class="hidden text-xs font-medium text-accent-600 group-hover:block dark:text-accent-400">Use</span>
              </button>
            </div>
            <div v-if="provider === 'ollama'" class="flex items-center gap-2 border-t border-zinc-100 p-3 dark:border-zinc-800">
              <Download class="size-4 shrink-0 text-zinc-400" />
              <input v-model="pullName" class="input !py-1.5" :placeholder="tab === 'embed' ? 'Download a model, e.g. mxbai-embed-large' : 'Download a model, e.g. gemma3:12b or qwen3:4b'" @keydown.enter="pull" />
              <button class="btn-outline" :disabled="!pullName.trim()" @click="pull">Download</button>
              <a href="https://ollama.com/library" target="_blank" class="shrink-0 text-xs text-accent-600 hover:underline dark:text-accent-400">Browse</a>
            </div>
          </div>
        </div>
        <div v-else class="grid flex-1 place-items-center"><Loader2 class="size-5 animate-spin text-zinc-400" /></div>
      </div>
    </div>
  </Teleport>
</template>
