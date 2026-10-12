<script setup>
import { onMounted, onUnmounted, watch, defineAsyncComponent } from 'vue';
import { store, ns, activeTab, loadAll, connectEvents, openTab, resolveNote, toast, refreshDue } from './store.js';
const ReviewView = defineAsyncComponent(() => import('./components/ReviewView.vue'));
import { api, isDesktop } from './api.js';
import Sidebar from './components/Sidebar.vue';
import TopBar from './components/TopBar.vue';
import ChatView from './components/ChatView.vue';
import NoteView from './components/NoteView.vue';
import StudioPanel from './components/StudioPanel.vue';
import CommandPalette from './components/CommandPalette.vue';
import Toasts from './components/Toasts.vue';
import ModelsDialog from './components/ModelsDialog.vue';
import Welcome from './components/Welcome.vue';
const GraphView = defineAsyncComponent(() => import('./components/GraphView.vue'));

// Citation chips anywhere in the app open the cited note in a tab (selection stays put).
function onClick(e) {
  const el = e.target.closest('.cite');
  if (!el || el.closest('.cm-editor')) return;
  e.preventDefault();
  const path = el.dataset.path || resolveNote(el.dataset.note);
  if (!path) return toast(`Couldn't find “${el.dataset.note}”`, 'error');
  openTab(path, el.dataset.heading || undefined);
}

// First-run / health check: if the local model server isn't reachable, say so with a fix.
async function checkModels() {
  if (!store.info.vault || store.info.provider !== 'ollama') return;
  try { await api('/api/models/list?provider=ollama'); }
  catch (e) { toast('Ollama isn’t running, so chat and search are paused. Start Ollama, or choose a cloud model.', 'error', { label: 'Set up models', run: () => (store.modelsOpen = true) }); }
}

function onKey(e) {
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key === 'k') { e.preventDefault(); store.paletteOpen = !store.paletteOpen; }
  if (mod && e.key === '.') { e.preventDefault(); store.studioOpen = !store.studioOpen; }
  if (mod && e.key === 'w' && ns.value && ns.value.active !== 'chat') { e.preventDefault(); const id = ns.value.active; import('./store.js').then((m) => m.closeTab(id)); }
}

// Native menu → UI actions (desktop app).
async function listenMenu() {
  if (!isDesktop) return;
  const { listen } = await import('@tauri-apps/api/event');
  const { pickFolder, pickNewFolder } = await import('./native.js');
  const { toggleDark, closeTab } = await import('./store.js');
  await listen('menu', async ({ payload: { id } }) => {
    if (id === 'search') store.paletteOpen = true;
    else if (id === 'review') store.reviewOpen = !store.reviewOpen;
    else if (id === 'toggle-studio') store.studioOpen = !store.studioOpen;
    else if (id === 'toggle-theme') toggleDark();
    else if (id === 'models') store.modelsOpen = true;
    else if (id === 'close-tab') { if (ns.value && ns.value.active !== 'chat') closeTab(ns.value.active); }
    else if (id === 'open-vault' || id === 'new-vault') {
      const p = id === 'open-vault' ? await pickFolder('Open a folder as a vault') : await pickNewFolder('Create a new vault');
      if (!p) return;
      try { await api(id === 'open-vault' ? '/api/vaults/select' : '/api/vaults/create', { path: p }); await loadAll(); toast(`Opened ${store.info.vaultName}`, 'success'); }
      catch (e) { toast(e.message, 'error'); }
    }
  });
}

onMounted(async () => {
  listenMenu();
  await loadAll().catch((e) => { if (e.status === 412) store.ready = true; else toast(e.message, 'error'); });
  connectEvents();
  refreshDue();
  checkModels();
  setInterval(refreshDue, 60000);
  document.addEventListener('click', onClick);
  document.addEventListener('keydown', onKey);
});
onUnmounted(() => { document.removeEventListener('click', onClick); document.removeEventListener('keydown', onKey); });
watch(() => store.info.vault, (v, old) => old && connectEvents());
</script>

<template>
  <Welcome v-if="store.ready && !store.info.vault" />
  <div v-else class="flex h-full overflow-hidden">
    <Sidebar />
    <main class="flex min-w-0 flex-1 flex-col">
      <TopBar />
      <div class="min-h-0 flex-1">
        <ReviewView v-if="store.reviewOpen" />
        <KeepAlive v-else :max="12">
          <ChatView v-if="!activeTab || activeTab.kind === 'chat'" :key="'chat:' + store.notebook" />
          <GraphView v-else-if="activeTab.kind === 'graph'" :key="'graph:' + store.notebook" />
          <NoteView v-else :key="store.notebook + ':' + activeTab.path" :path="activeTab.path" />
        </KeepAlive>
      </div>
    </main>
    <Transition enter-from-class="translate-x-full opacity-0" leave-to-class="translate-x-full opacity-0" enter-active-class="transition duration-200" leave-active-class="transition duration-150">
      <StudioPanel v-if="store.studioOpen && store.notebook" />
    </Transition>
    <CommandPalette />
    <Toasts />
    <ModelsDialog :open="store.modelsOpen" @close="store.modelsOpen = false" />
  </div>
</template>
