<script setup>
import { Loader2 } from 'lucide-vue-next';
defineProps({ label: String });
</script>
<template>
  <div class="flex flex-col items-center gap-3 py-12 text-center">
    <div class="relative grid size-12 place-items-center">
      <div class="absolute inset-0 animate-ping rounded-full bg-accent-500/20" />
      <Loader2 class="size-6 animate-spin text-accent-500" />
    </div>
    <div class="text-sm font-medium">{{ label }}</div>
    <div class="text-xs text-zinc-400">Running locally. This can take a minute.</div>
  </div>
</template>
