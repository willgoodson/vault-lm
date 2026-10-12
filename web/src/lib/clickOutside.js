import { onMounted, onUnmounted } from 'vue';
export function onClickOutside(elRef, fn) {
  const h = (e) => elRef.value && !elRef.value.contains(e.target) && fn();
  onMounted(() => document.addEventListener('mousedown', h));
  onUnmounted(() => document.removeEventListener('mousedown', h));
}
