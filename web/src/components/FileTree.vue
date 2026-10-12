<script setup>
// Raw vault browser. Opening a file only opens a tab — it never changes the notebook/section/topic selection.
import { ref, computed } from 'vue';
import { ChevronRight, FileText, Folder, FileAudio, FileImage, File, Sparkles } from 'lucide-vue-next';
import { activeTab, openTab } from '../store.js';

const props = defineProps({ nodes: Array, filter: String, depth: { type: Number, default: 0 } });
const collapsed = ref({});

const matches = (n) => !props.filter || (n.type === 'file' ? n.path.toLowerCase().includes(props.filter) : n.children.some(matches));
const visible = computed(() => props.nodes.filter(matches));
const isOpen = (n) => props.filter || !(collapsed.value[n.path] ?? (n.name === '_studio' || props.depth > 0));
const icon = (n) => (n.name.endsWith('.md') ? FileText : /\.(wav|mp3|m4a)$/.test(n.name) ? FileAudio : /\.(png|jpe?g|gif|webp|svg)$/i.test(n.name) ? FileImage : File);
</script>

<template>
  <ul>
    <li v-for="n in visible" :key="n.path">
      <button v-if="n.type === 'dir'" class="flex w-full items-center gap-1 rounded-md py-1 pr-2 text-left text-[13px] text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
        :style="{ paddingLeft: `${depth * 12 + 6}px` }" @click="collapsed[n.path] = isOpen(n)">
        <ChevronRight class="size-3.5 shrink-0 transition" :class="isOpen(n) && 'rotate-90'" />
        <Sparkles v-if="n.name === '_studio'" class="size-3.5 shrink-0 text-accent-500" />
        <Folder v-else class="size-3.5 shrink-0 opacity-60" />
        <span class="truncate">{{ n.name === '_studio' ? 'Studio outputs' : n.name }}</span>
      </button>
      <button v-else class="flex w-full min-w-0 items-center gap-1.5 rounded-md py-1 text-left text-[13px] transition"
        :class="activeTab?.path === n.path ? 'bg-zinc-200/60 text-zinc-900 dark:bg-zinc-800 dark:text-white' : 'text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800'"
        :style="{ paddingLeft: `${depth * 12 + 22}px` }" @click="openTab(n.path)">
        <component :is="icon(n)" class="size-3.5 shrink-0 opacity-60" />
        <span class="truncate">{{ n.name.replace(/\.md$/, '') }}</span>
      </button>
      <FileTree v-if="n.type === 'dir' && isOpen(n)" :nodes="n.children" :filter="filter" :depth="depth + 1" />
    </li>
  </ul>
</template>
