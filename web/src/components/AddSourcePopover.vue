<script setup>
import { ref, computed } from 'vue';
import { Link2, FileUp, Loader2 } from 'lucide-vue-next';
import Popover from './Popover.vue';
import { toast, scopeName } from '../store.js';
import { api } from '../api.js';

// target: { notebook, section?, topic? }
const props = defineProps({ anchor: Object, open: Boolean, target: Object, label: String });
const emit = defineEmits(['close']);
const url = ref('');
const busy = ref(false);
const where = computed(() => props.label || scopeName(props.target || {}));
const ACCEPT = '.pdf,.epub,.png,.jpg,.jpeg,.webp,.gif,.bmp,.tif,.tiff,.txt,.md,.markdown,.html,.htm';
const isYouTube = computed(() => /youtu\.?be/.test(url.value));

async function addUrl() {
  busy.value = true;
  try {
    const r = await api('/api/source/url', { ...props.target, url: url.value });
    toast(`Added “${r.path.split('/').pop().replace(/\.md$/, '')}”`, 'success');
    url.value = '';
    emit('close');
  } catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}

async function addFiles(fileList) {
  const fd = new FormData();
  for (const f of fileList) if (ACCEPT.split(',').some((x) => f.name.toLowerCase().endsWith(x))) fd.append('file', f, f.name);
  if (!fd.has('file')) return toast('Supported: PDF, EPUB, images, .txt/.md/.html', 'error');
  busy.value = true;
  try {
    const q = new URLSearchParams(Object.entries(props.target).filter(([, v]) => v));
    const r = await api(`/api/source/file?${q}`, fd);
    toast(`Added ${r.paths.length} source${r.paths.length > 1 ? 's' : ''}${r.failed?.length ? ` · ${r.failed.length} failed` : ''}`, r.failed?.length ? 'error' : 'success');
    emit('close');
  } catch (e) { toast(e.message, 'error'); }
  busy.value = false;
}
</script>

<template>
  <Popover :anchor="anchor" :open="open" :width="300" @close="emit('close')">
    <div class="px-1.5 pt-1 pb-2 text-xs text-zinc-500">Add a source to <span class="font-semibold text-zinc-800 dark:text-zinc-100">{{ where }}</span></div>
    <p v-if="isYouTube" class="px-1.5 pb-1.5 text-[11px] text-accent-600 dark:text-accent-400">YouTube link: the video's transcript will be imported.</p>
    <form class="flex gap-1.5 px-1" @submit.prevent="addUrl">
      <div class="relative flex-1">
        <Link2 class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-zinc-400" />
        <input v-model="url" type="url" required autofocus class="input !rounded-lg !py-1.5 !pl-8 text-[13px]" placeholder="Web page or YouTube URL" />
      </div>
      <button class="btn-primary !px-2.5" :disabled="busy">Add</button>
    </form>
    <label class="m-1 mt-2 flex cursor-pointer flex-col items-center gap-1 rounded-xl border border-dashed border-zinc-300 py-3 text-xs text-zinc-500 transition hover:border-accent-400 hover:bg-accent-500/5 dark:border-zinc-700"
      @dragover.prevent @drop.prevent="addFiles($event.dataTransfer.files)">
      <Loader2 v-if="busy" class="size-4 animate-spin" /><FileUp v-else class="size-4" />
      {{ busy ? 'Converting…' : 'Drop files or click to choose' }}
      <span v-if="!busy" class="text-[10px] text-zinc-400">PDF (scans are OCR'd) · EPUB · images · text</span>
      <input type="file" :accept="ACCEPT" multiple hidden @change="addFiles($event.target.files)" />
    </label>
    <p class="px-1.5 pb-1 text-[11px] leading-snug text-zinc-400">
      {{ target?.topic ? 'Saved beside this topic in sources/. Your note isn’t changed.' : 'Saved to this folder’s sources/ and indexed automatically.' }}
    </p>
  </Popover>
</template>
