<script setup>
// Floating panel anchored to an element; teleported so scroll containers don't clip it.
import { ref, watch, nextTick, onMounted, onUnmounted } from 'vue';
const props = defineProps({ anchor: Object, open: Boolean, width: { type: Number, default: 260 }, align: { type: String, default: 'start' } });
const emit = defineEmits(['close']);
const el = ref(null);
const pos = ref({ top: 0, left: 0 });

function place() {
  const a = props.anchor?.getBoundingClientRect?.();
  if (!a) return;
  let left = props.align === 'end' ? a.right - props.width : a.left;
  left = Math.max(8, Math.min(left, innerWidth - props.width - 8));
  let top = a.bottom + 6;
  const h = el.value?.offsetHeight || 0;
  if (top + h > innerHeight - 8) top = Math.max(8, a.top - h - 6);
  pos.value = { top, left };
}
watch(() => props.open, (o) => o && nextTick(place));
const onDown = (e) => props.open && el.value && !el.value.contains(e.target) && !props.anchor?.contains?.(e.target) && emit('close');
const onKey = (e) => props.open && e.key === 'Escape' && emit('close');
onMounted(() => { document.addEventListener('mousedown', onDown); document.addEventListener('keydown', onKey); addEventListener('resize', place); });
onUnmounted(() => { document.removeEventListener('mousedown', onDown); document.removeEventListener('keydown', onKey); removeEventListener('resize', place); });
</script>

<template>
  <Teleport to="body">
    <Transition enter-from-class="opacity-0 -translate-y-1" leave-to-class="opacity-0" enter-active-class="transition duration-150" leave-active-class="transition duration-100">
      <div v-if="open" ref="el" class="panel fixed z-50 p-1.5 shadow-xl" :style="{ top: pos.top + 'px', left: pos.left + 'px', width: width + 'px' }">
        <slot />
      </div>
    </Transition>
  </Teleport>
</template>
