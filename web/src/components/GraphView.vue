<script setup>
import { ref, onMounted, onUnmounted, watch, onActivated } from 'vue';
import ForceGraph from 'force-graph';
import { Sparkles, Maximize, Loader2 } from 'lucide-vue-next';
import { store, currentNb, openTab } from '../store.js';
import { api } from '../api.js';

const host = ref(null);
const similar = ref(localStorage.vlmGraphSimilar === '1');
const loading = ref(false);
const stats = ref({ nodes: 0, links: 0, similar: 0 });
let graph, ro, fitted = false;
// Fit once the layout settles; don't zoom in absurdly on tiny notebooks.
function fit() { graph.zoomToFit(400, 80); setTimeout(() => graph.zoom() > 2.2 && graph.zoom(2.2, 300), 450); }
const PALETTE = ['#8b5cf6', '#06b6d4', '#10b981', '#f59e0b', '#ec4899', '#3b82f6', '#ef4444', '#84cc16'];

async function load() {
  const nb = currentNb.value;
  if (!nb || !graph) return;
  loading.value = true;
  const g = await api(`/api/graph?notebook=${encodeURIComponent(nb.path)}${similar.value ? '&similar=1' : ''}`);
  const secIdx = Object.fromEntries(nb.sections.map((s, i) => [s.path, i]));
  const sectionOf = (p) => nb.sections.find((s) => p.startsWith(s.path + '/'));
  const degree = {};
  for (const e of g.edges) { degree[e.source] = (degree[e.source] || 0) + 1; degree[e.target] = (degree[e.target] || 0) + 1; }
  const nodes = g.nodes.map((p) => {
    const sec = sectionOf(p);
    return { id: p, name: p.split('/').pop().replace(/\.md$/, ''), section: sec?.name || nb.name, color: sec ? PALETTE[secIdx[sec.path] % PALETTE.length] : '#71717a', val: 1 + (degree[p] || 0), source: p.includes('/sources/') };
  });
  const links = [...g.edges.map((e) => ({ ...e, kind: 'link' })), ...g.similar.map((e) => ({ ...e, kind: 'similar' }))];
  stats.value = { nodes: nodes.length, links: g.edges.length, similar: g.similar.length };
  graph.graphData({ nodes, links });
  fitted = false;
  loading.value = false;
}

function draw(node, ctx, scale) {
  const r = 3 + Math.sqrt(node.val) * 2;
  ctx.beginPath();
  ctx.arc(node.x, node.y, r, 0, 2 * Math.PI);
  ctx.fillStyle = node.color;
  ctx.globalAlpha = node.source ? 0.55 : 1;
  ctx.fill();
  ctx.globalAlpha = 1;
  if (scale > 0.7 || node.val > 2) {
    ctx.font = `${12 / scale}px Inter Variable, sans-serif`;
    ctx.textAlign = 'center';
    ctx.fillStyle = store.dark ? '#e4e4e7' : '#3f3f46';
    ctx.fillText(node.name, node.x, node.y + r + 12 / scale);
  }
}

onMounted(() => {
  graph = ForceGraph()(host.value)
    .backgroundColor('rgba(0,0,0,0)')
    .nodeCanvasObject(draw)
    .nodePointerAreaPaint((n, color, ctx) => { ctx.fillStyle = color; ctx.beginPath(); ctx.arc(n.x, n.y, 8, 0, 2 * Math.PI); ctx.fill(); })
    .nodeLabel((n) => `${n.name} · ${n.section}`)
    .linkColor((l) => (l.kind === 'similar' ? 'rgba(139,92,246,0.45)' : store.dark ? 'rgba(161,161,170,0.45)' : 'rgba(113,113,122,0.35)'))
    .linkLineDash((l) => (l.kind === 'similar' ? [3, 3] : null))
    .linkWidth((l) => (l.kind === 'similar' ? 1 : 1.5))
    .linkDirectionalArrowLength((l) => (l.kind === 'link' ? 4 : 0))
    .linkDirectionalArrowRelPos(1)
    .cooldownTime(2500)
    .onEngineStop(() => { if (!fitted) { fitted = true; fit(); } })
    .onNodeClick((n) => openTab(n.id));
  ro = new ResizeObserver(() => graph.width(host.value.clientWidth).height(host.value.clientHeight));
  ro.observe(host.value);
  load();
});
onActivated(load);
onUnmounted(() => { ro?.disconnect(); graph?._destructor?.(); });
watch(similar, (v) => { localStorage.vlmGraphSimilar = v ? '1' : '0'; load(); });
watch(() => store.notebooks, load);
</script>

<template>
  <div class="relative h-full">
    <div ref="host" class="absolute inset-0" />
    <div class="panel absolute top-4 left-4 flex items-center gap-3 px-3 py-2 text-xs">
      <div class="font-semibold">{{ currentNb?.name }} graph</div>
      <div class="text-zinc-400">{{ stats.nodes }} notes · {{ stats.links }} links<span v-if="similar"> · {{ stats.similar }} similar</span></div>
      <label class="flex cursor-pointer items-center gap-1.5 text-zinc-600 dark:text-zinc-300">
        <input v-model="similar" type="checkbox" class="accent-accent-500" /><Sparkles class="size-3 text-accent-500" /> Show similar notes
      </label>
      <Loader2 v-if="loading" class="size-3.5 animate-spin text-zinc-400" />
      <button class="btn-ghost !p-1" title="Fit" @click="fit()"><Maximize class="size-3.5" /></button>
    </div>
    <div v-if="currentNb?.sections.length" class="panel absolute bottom-4 left-4 flex flex-wrap gap-x-3 gap-y-1 px-3 py-2 text-[11px]">
      <span v-for="(s, i) in currentNb.sections" :key="s.path" class="flex items-center gap-1"><span class="size-2 rounded-full" :style="{ background: PALETTE[i % PALETTE.length] }" />{{ s.name }}</span>
      <span class="flex items-center gap-1 text-zinc-400"><span class="size-2 rounded-full bg-zinc-500" />notebook level</span>
    </div>
  </div>
</template>
