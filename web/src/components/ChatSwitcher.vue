<script setup>
import { ref } from 'vue';
import { MessagesSquare, ChevronDown, Plus, Pencil, Trash2, Check } from 'lucide-vue-next';
import { ns, currentChat, addChat, deleteChat } from '../store.js';
import Popover from './Popover.vue';
import InlineInput from './InlineInput.vue';

const btn = ref(null);
const open = ref(false);
const renaming = ref(null);
const when = (t) => new Date(t).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
function pick(c) { ns.value.activeChat = c.id; open.value = false; }
function rename(c, v) { c.title = v; c.titled = true; renaming.value = null; }
function remove(c) { if (!c.messages.length || confirm(`Delete “${c.title}”?`)) deleteChat(c.id); }
</script>

<template>
  <div class="flex items-center gap-1">
    <button ref="btn" class="btn-ghost max-w-64 !gap-1.5 !px-2 !py-1 text-sm" @click="open = !open">
      <MessagesSquare class="size-4 shrink-0 text-zinc-400" /><span class="truncate font-medium">{{ currentChat?.title }}</span><ChevronDown class="size-3.5 shrink-0 text-zinc-400" />
    </button>
    <button class="btn-ghost !p-1.5" title="New chat" @click="addChat"><Plus class="size-4" /></button>
    <Popover :anchor="btn" :open="open" :width="300" @close="open = false; renaming = null">
      <button class="menu-item" @click="addChat(); open = false"><Plus class="size-3.5" /> New chat</button>
      <div class="my-1 h-px bg-zinc-100 dark:bg-zinc-800" />
      <div class="max-h-72 overflow-y-auto">
        <div v-for="c in ns.chats" :key="c.id" class="group flex items-center gap-1 rounded-lg px-1 hover:bg-zinc-100 dark:hover:bg-zinc-800">
          <InlineInput v-if="renaming === c.id" :value="c.title" @commit="(v) => rename(c, v)" @cancel="renaming = null" />
          <button v-else class="flex min-w-0 flex-1 items-center gap-2 px-1 py-1.5 text-left text-sm" @click="pick(c)">
            <Check class="size-3.5 shrink-0" :class="c.id === currentChat?.id ? 'text-accent-500' : 'invisible'" />
            <span class="min-w-0 flex-1 truncate">{{ c.title }}</span>
            <span class="shrink-0 text-[10px] text-zinc-400">{{ c.messages.length ? `${c.messages.length} · ` : '' }}{{ when(c.created) }}</span>
          </button>
          <button v-if="renaming !== c.id" class="hidden rounded p-1 text-zinc-400 group-hover:block hover:text-zinc-700" title="Rename" @click="renaming = c.id"><Pencil class="size-3" /></button>
          <button v-if="renaming !== c.id" class="hidden rounded p-1 text-zinc-400 group-hover:block hover:text-red-600" title="Delete" @click="remove(c)"><Trash2 class="size-3" /></button>
        </div>
      </div>
    </Popover>
  </div>
</template>
