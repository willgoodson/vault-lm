<script setup>
import { ChevronRight, Paperclip } from 'lucide-vue-next';
import { COLORS } from '../lib/colors.js';
import InlineInput from './InlineInput.vue';
defineProps({
  depth: { type: Number, default: 0 }, label: String, icon: [Object, Function], iconClass: String,
  state: String,            // 'on' | 'parent' | undefined
  expandable: Boolean, expanded: Boolean, renaming: Boolean, count: [Number, String], clips: Number, muted: Boolean, meta: Object,
});
defineEmits(['select', 'toggle', 'rename', 'cancel-rename']);
</script>

<template>
  <div class="group relative flex items-center rounded-lg transition" :title="meta?.description || undefined"
    :style="meta?.status === 'done' || meta?.status === 'paused' ? 'opacity: .55' : undefined"
    :class="state === 'on' ? 'bg-accent-500/10 text-accent-700 dark:text-accent-300' : state === 'parent' ? 'text-zinc-900 dark:text-zinc-100' : muted ? 'text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800' : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800'">
    <div class="flex min-w-0 flex-1 items-center gap-1.5 py-1 pr-1" :style="{ paddingLeft: `${depth * 14 + 4}px` }">
      <button class="grid size-4 shrink-0 place-items-center rounded hover:bg-zinc-200 dark:hover:bg-zinc-700" :class="!expandable && 'invisible'" @click.stop="$emit('toggle')">
        <ChevronRight class="size-3 transition" :class="expanded && 'rotate-90'" />
      </button>
      <component :is="icon" class="size-3.5 shrink-0" :class="iconClass || 'opacity-60'" :style="meta?.color ? { color: COLORS[meta.color], opacity: 1 } : undefined" />
      <InlineInput v-if="renaming" :value="label" @commit="(v) => $emit('rename', v)" @cancel="$emit('cancel-rename')" />
      <button v-else class="min-w-0 flex-1 truncate text-left text-[13px]" :class="state === 'on' && 'font-medium'" @click="$emit('select')">{{ label }}</button>
      <span v-if="meta?.status && meta.status !== 'active' && !renaming" class="chip shrink-0 bg-zinc-100 !px-1.5 !py-0 text-[10px] text-zinc-500 dark:bg-zinc-800">{{ meta.status }}</span>
      <span v-if="meta?.due && !renaming" class="shrink-0 text-[10px] text-zinc-400 group-hover:hidden" :title="`Due ${meta.due}`">{{ new Date(meta.due + 'T00:00').toLocaleDateString(undefined, { month: 'short', day: 'numeric' }) }}</span>
      <span v-if="clips && !renaming" class="flex items-center gap-0.5 text-[10px] text-zinc-400 group-hover:hidden"><Paperclip class="size-2.5" />{{ clips }}</span>
      <span v-else-if="count && !renaming" class="text-[10px] text-zinc-400 group-hover:hidden">{{ count }}</span>
    </div>
    <div v-if="!renaming" class="mr-1 hidden shrink-0 items-center gap-px group-hover:flex [&:has([data-open])]:flex">
      <slot name="actions" />
    </div>
  </div>
</template>
