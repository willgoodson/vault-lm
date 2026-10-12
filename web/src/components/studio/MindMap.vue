<script setup>
import { ref, nextTick, onUnmounted } from 'vue';
import { Transformer } from 'markmap-lib';
import { Markmap } from 'markmap-view';
import { Sparkles, ExternalLink, Maximize, RefreshCw } from 'lucide-vue-next';
import { scope, toast, openTab, resolveNote } from '../../store.js';
import { api } from '../../api.js';
import Generating from './Generating.vue';

const sc = scope();
const busy = ref(false);
const result = ref(null);
const svg = ref(null);
let mm;
const transformer = new Transformer();
const esc = (s) => s.replace(/"/g, '&quot;');

// [[Note#Heading]] → clickable chip (handled by the app-wide .cite click handler).
const withCites = (md) => md.replace(/\[\[([^\]|#]+)(?:#([^\]|]+))?\]\]/g, (_, n, h) =>
  `<a class="cite" data-note="${esc(n.trim())}" data-path="${esc(resolveNote(n.trim()) || '')}" data-heading="${esc(h || '')}">↗</a>`);

async function generate() {
  busy.value = true;
  try {
    result.value = await api('/api/study/mindmap', sc);
    busy.value = false;
    await nextTick();
    const { root } = transformer.transform(withCites(result.value.markdown));
    mm?.destroy();
    mm = Markmap.create(svg.value, { duration: 300, maxWidth: 260, paddingX: 12, spacingVertical: 8 }, root);
  } catch (e) { toast(e.message, 'error'); busy.value = false; }
}
onUnmounted(() => mm?.destroy());
</script>

<template>
  <div class="flex h-full flex-col p-4">
    <Generating v-if="busy" label="Mapping concepts…" />
    <div v-else-if="!result" class="panel p-5 text-center">
      <div class="text-sm font-medium">Mind map</div>
      <p class="mt-1 text-xs text-zinc-500">A visual outline of the key concepts. Click ↗ on any node to open its source note. Click a dot to collapse that branch.</p>
      <button class="btn-primary mt-4 w-full !py-2.5" @click="generate"><Sparkles class="size-4" /> Generate mind map</button>
    </div>
    <template v-if="result && !busy">
      <div class="mb-2 flex gap-1.5">
        <button class="btn-outline" @click="mm?.fit()"><Maximize class="size-3.5" /> Fit</button>
        <button class="btn-outline" @click="generate"><RefreshCw class="size-3.5" /> Regenerate</button>
        <button class="btn-ghost ml-auto" @click="openTab(result.path)"><ExternalLink class="size-3.5" /> Obsidian</button>
      </div>
    </template>
    <div v-show="result && !busy" class="panel min-h-[480px] flex-1 overflow-hidden">
      <svg ref="svg" class="markmap size-full" />
    </div>
  </div>
</template>

<style>
.markmap { --markmap-font: 400 13px/18px "Inter Variable", sans-serif; --markmap-text-color: #3f3f46; }
.dark .markmap { --markmap-text-color: #e4e4e7; --markmap-circle-open-bg: #27272a; }
.markmap .cite { margin-left: 4px; padding: 0 5px; font-size: 11px; }
</style>
