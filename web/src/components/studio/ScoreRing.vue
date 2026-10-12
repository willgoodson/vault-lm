<script setup>
import { computed } from 'vue';
const props = defineProps({ value: Number, max: Number, size: { type: Number, default: 96 } });
const pct = computed(() => Math.max(0, Math.min(1, props.value / props.max)));
const color = computed(() => (pct.value >= 0.8 ? '#10b981' : pct.value >= 0.5 ? '#f59e0b' : '#ef4444'));
const C = 2 * Math.PI * 42;
</script>
<template>
  <div class="relative" :style="{ width: size + 'px', height: size + 'px' }">
    <svg viewBox="0 0 100 100" class="-rotate-90">
      <circle cx="50" cy="50" r="42" fill="none" stroke="currentColor" stroke-width="8" class="text-zinc-100 dark:text-zinc-800" />
      <circle cx="50" cy="50" r="42" fill="none" :stroke="color" stroke-width="8" stroke-linecap="round" :stroke-dasharray="C" :stroke-dashoffset="C * (1 - pct)" class="transition-all duration-700" />
    </svg>
    <div class="absolute inset-0 grid place-items-center text-center">
      <div><div class="text-2xl font-bold tracking-tight">{{ value }}</div><div class="-mt-1 text-[11px] text-zinc-400">of {{ max }}</div></div>
    </div>
  </div>
</template>
