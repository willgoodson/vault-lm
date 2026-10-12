<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { AudioLines, ExternalLink } from 'lucide-vue-next';
import { scope, toast, openTab, fileUrl } from '../../store.js';
import { api } from '../../api.js';
import Generating from './Generating.vue';

const sc = scope();
const job = ref({ status: 'idle' });
const v = ref(0);
const exists = ref(true);
// Mirrors server studioPath(): <notebook>/_studio/[<note> - ]Audio Overview.wav
const wav = computed(() => job.value.path || (sc.topic
  ? `${sc.topic.split('/').slice(0, -1).join('/')}/_studio/${sc.topic.split('/').pop().replace(/\.md$/, '')} - Audio Overview.wav`
  : `${sc.section || sc.notebook}/_studio/Audio Overview.wav`));
const src = computed(() => `${fileUrl(wav.value)}${fileUrl(wav.value).includes('?') ? '&' : '?'}v=${v.value}`);
const running = computed(() => !['idle', 'done', 'error'].includes(job.value.status));
let timer;

async function poll() {
  job.value = await api('/api/audio/status', sc);
  if (job.value.status === 'done') { v.value++; exists.value = true; stop(); }
  if (job.value.status === 'error') { toast(job.value.error, 'error'); stop(); }
}
const stop = () => clearInterval(timer);
async function start() {
  job.value = await api('/api/audio', sc);
  timer = setInterval(poll, 1500);
}
onMounted(async () => { await poll(); if (running.value) timer = setInterval(poll, 1500); });
onUnmounted(stop);
</script>

<template>
  <div class="p-4">
    <div class="overflow-hidden rounded-2xl bg-gradient-to-br from-indigo-500 to-fuchsia-500 p-5 text-white shadow-lg shadow-indigo-500/20">
      <AudioLines class="size-7" />
      <div class="mt-3 text-lg font-semibold">Audio overview</div>
      <p class="text-sm text-white/80">Two AI hosts talk through your notes like a podcast. The voices are generated locally with Kokoro.</p>
      <button class="mt-4 w-full rounded-xl bg-white/95 py-2.5 text-sm font-semibold text-indigo-700 shadow-sm transition hover:bg-white disabled:opacity-60" :disabled="running" @click="start">
        {{ running ? 'Generating…' : exists ? 'Regenerate' : 'Generate' }}
      </button>
    </div>
    <Generating v-if="running" :label="job.status" />
    <div v-show="exists && !running" class="panel mt-4 p-3">
      <audio :key="v" controls class="w-full" :src="src" @error="exists = false" />
      <button class="btn-ghost mt-1 w-full text-xs" @click="openTab(wav.replace(/\.wav$/, '.md'))"><ExternalLink class="size-3" /> Open transcript in Obsidian</button>
    </div>
  </div>
</template>
