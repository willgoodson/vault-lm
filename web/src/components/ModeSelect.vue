<script setup>
import { ref } from 'vue';
import { ChevronDown, Check, Telescope, Crosshair } from 'lucide-vue-next';
import { currentChat, MODES } from '../store.js';
import Popover from './Popover.vue';
const btn = ref(null);
const open = ref(false);
const icons = { reference: Telescope, focused: Crosshair };
const current = () => MODES.find((m) => m.id === currentChat.value?.mode) || MODES[0];
</script>
<template>
  <button ref="btn" class="btn-outline !gap-1 !rounded-lg !py-1 text-xs" @click="open = !open">
    <component :is="icons[current().id]" class="size-3.5 text-accent-500" />{{ current().label }}<ChevronDown class="size-3 text-zinc-400" />
  </button>
  <Popover :anchor="btn" :open="open" :width="300" @close="open = false">
    <button v-for="m in MODES" :key="m.id" class="flex w-full items-start gap-2.5 rounded-lg p-2 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800" @click="currentChat.mode = m.id; open = false">
      <component :is="icons[m.id]" class="mt-0.5 size-4 shrink-0 text-accent-500" />
      <div class="min-w-0 flex-1">
        <div class="text-sm font-medium">{{ m.label }}<span v-if="m.id === 'reference'" class="ml-1.5 text-[10px] font-normal text-zinc-400">default</span></div>
        <div class="text-xs leading-snug text-zinc-500">{{ m.hint }}</div>
      </div>
      <Check v-if="currentChat?.mode === m.id" class="mt-0.5 size-4 text-accent-500" />
    </button>
  </Popover>
</template>
