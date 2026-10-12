<script setup>
import { CheckCircle2, AlertCircle, Info } from 'lucide-vue-next';
import { store, dismissToast } from '../store.js';
const icons = { success: CheckCircle2, error: AlertCircle, info: Info };
const colors = { success: 'text-emerald-500', error: 'text-red-500', info: 'text-accent-500' };
</script>
<template>
  <div class="pointer-events-none fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2">
    <div v-for="j in store.jobs" :key="j.id" class="panel pointer-events-auto w-72 px-4 py-2.5 text-sm shadow-lg">
      <div class="flex justify-between gap-2"><span class="truncate font-medium">{{ j.label }}</span><span class="shrink-0 text-xs text-zinc-400">{{ j.note }}</span></div>
      <div class="mt-1.5 h-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800"><div class="h-full rounded-full bg-accent-500 transition-all" :style="{ width: `${Math.round((j.done / Math.max(1, j.total)) * 100)}%` }" /></div>
    </div>
    <TransitionGroup enter-from-class="opacity-0 translate-y-2" leave-to-class="opacity-0" enter-active-class="transition" leave-active-class="transition">
      <div v-for="t in store.toasts" :key="t.id" class="panel pointer-events-auto flex items-center gap-2 px-4 py-2.5 text-sm shadow-lg">
        <component :is="icons[t.kind]" class="size-4" :class="colors[t.kind]" /> {{ t.msg }}
        <button v-if="t.action" class="ml-2 rounded-md px-2 py-0.5 text-xs font-semibold text-accent-600 hover:bg-accent-500/10 dark:text-accent-400" @click="t.action.run(); dismissToast(t.id)">{{ t.action.label }}</button>
      </div>
    </TransitionGroup>
  </div>
</template>
