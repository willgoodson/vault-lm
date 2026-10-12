<script setup>
import { ref, onMounted } from 'vue';
const props = defineProps({ value: { type: String, default: '' }, placeholder: String });
const emit = defineEmits(['commit', 'cancel']);
const v = ref(props.value);
const el = ref(null);
let done = false;
onMounted(() => { el.value.focus(); el.value.select(); });
const commit = () => { if (done) return; done = true; v.value.trim() && v.value.trim() !== props.value ? emit('commit', v.value.trim()) : emit('cancel'); };
const cancel = () => { if (done) return; done = true; emit('cancel'); };
</script>
<template>
  <input ref="el" v-model="v" :placeholder="placeholder" @keydown.enter.prevent="commit" @keydown.esc.prevent="cancel" @blur="commit"
    class="w-full min-w-0 rounded-md border border-accent-400 bg-white px-1.5 py-0.5 text-[13px] outline-none ring-4 ring-accent-500/10 dark:bg-zinc-900" />
</template>
