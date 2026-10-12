import { reactive, computed, watch } from 'vue';
import { api, isDesktop } from './api.js';
import { convertFileSrc } from '@tauri-apps/api/core';
import { openExternal } from './native.js';

/**
 * Global app state. Everything that belongs to a notebook (selection, tabs, chat, mode, studio)
 * lives in store.nb[<notebook path>] so switching notebooks swaps it all and switching back restores it.
 */
export const store = reactive({
  info: {},
  vaults: [],
  notebooks: [],          // tree: [{name, path, topics, sections: [{name, path, topics, sources}], sources}]
  notebook: null,         // current notebook path
  nb: {},                 // per-notebook state, see newNbState()
  layout: { root: '', folders: [] },
  templates: [],
  tree: [],               // raw vault folder tree (Vault files)
  palette: [],            // [{path, note, heading, tags}]
  index: { running: false, done: 0, total: 0 },
  overview: null,         // notebook path whose overview is being regenerated
  studioOpen: true,
  reviewOpen: false,      // vault-wide Review dashboard in the main area
  dueTotal: 0,
  paletteOpen: false,
  paletteQuery: '',
  modelsOpen: false,
  toasts: [],
  jobs: {},               // long-running imports (OCR, EPUB) reported over SSE
  composer: '',
  dark: document.documentElement.classList.contains('dark'),
  ready: false,
});

export const newChat = (n = 1) => ({ id: Math.random().toString(36).slice(2, 10), title: `Chat ${n}`, messages: [], mode: 'reference', created: Date.now(), titled: false });

// Older saved state had a single `chat` array + `mode` per notebook.
function migrate(s) {
  if (!s.chats) {
    const c = newChat();
    if (s.chat?.length) { c.messages = s.chat; c.mode = s.mode || 'reference'; }
    s.chats = [c];
    s.activeChat = c.id;
  }
  delete s.chat; delete s.mode;
  if (!s.chats.length) s.chats.push(newChat());
  if (!s.chats.some((c) => c.id === s.activeChat)) s.activeChat = s.chats[0].id;
  return s;
}

const newNbState = () => migrate({
  section: null,
  topic: null,
  tabs: [{ id: 'chat', kind: 'chat' }],
  active: 'chat',
  heading: null,          // heading to scroll to in the active note tab
  scroll: {},             // tabId -> scrollTop
  studio: { level: null, tool: null },
});

// Current notebook's state (created on demand).
export const ns = computed(() => {
  if (!store.notebook) return null;
  return (store.nb[store.notebook] ??= newNbState());
});

export const currentChat = computed(() => ns.value?.chats.find((c) => c.id === ns.value.activeChat) ?? ns.value?.chats[0]);
export function addChat() {
  const c = newChat(ns.value.chats.length + 1);
  ns.value.chats.unshift(c);
  ns.value.activeChat = c.id;
  ns.value.active = 'chat';
  return c;
}
export function deleteChat(id) {
  const s = ns.value;
  s.chats = s.chats.filter((c) => c.id !== id);
  if (!s.chats.length) s.chats.push(newChat());
  if (s.activeChat === id) s.activeChat = s.chats[0].id;
}

export const currentNb = computed(() => store.notebooks.find((n) => n.path === store.notebook));
export const currentSection = computed(() => currentNb.value?.sections.find((s) => s.path === ns.value?.section));
export const allTopics = (nb) => (nb ? [...nb.topics, ...nb.sections.flatMap((s) => s.topics)] : []);
export const currentTopic = computed(() => allTopics(currentNb.value).find((t) => t.path === ns.value?.topic));

// The selection made in the Notebooks tree: always rooted in the current notebook.
export const selection = computed(() => ({
  notebook: store.notebook,
  section: ns.value?.section || undefined,
  topic: ns.value?.topic || undefined,
}));

// Levels available for the current selection, deepest last: [{level, name, icon}]
export const levels = computed(() => {
  const out = [];
  if (currentNb.value) out.push({ level: 'notebook', name: currentNb.value.name, icon: '📓' });
  if (currentSection.value) out.push({ level: 'section', name: currentSection.value.name, icon: '#' });
  if (currentTopic.value) out.push({ level: 'topic', name: currentTopic.value.name.split('/').pop(), icon: '📄' });
  return out;
});
export const breadcrumb = computed(() => levels.value.map((l) => l.name).join(' › '));

// Scope object for a given level (defaults to the studio's chosen level, else the deepest).
export function scope(level = ns.value?.studio.level) {
  const avail = levels.value.map((l) => l.level);
  if (!avail.includes(level)) level = avail.at(-1);
  const s = { notebook: store.notebook };
  if (level === 'section' || level === 'topic') s.section = ns.value.section || undefined;
  if (level === 'topic') s.topic = ns.value.topic;
  return s;
}
export const scopeName = (s) => (s.topic ? s.topic.split('/').pop().replace(/\.md$/, '') : s.section ? s.section.split('/').pop() : s.notebook?.split('/').pop());

// ---- selection (only the Notebooks tree calls these)
export function selectNotebook(path) {
  store.reviewOpen = false;
  store.notebook = path;
}
export function select({ section = null, topic = null } = {}) {
  if (!ns.value) return;
  ns.value.section = section;
  ns.value.topic = topic;
}

// ---- tabs (never change the selection)
export function openTab(path, heading) {
  if (!ns.value) return;
  store.reviewOpen = false;
  if (!path.endsWith('.md')) return window.open(fileUrl(path), '_blank');
  if (!ns.value.tabs.some((t) => t.id === path)) ns.value.tabs.push({ id: path, kind: 'note', path });
  ns.value.active = path;
  ns.value.heading = heading || null;
}
export function openGraph() {
  if (!ns.value.tabs.some((t) => t.id === 'graph')) ns.value.tabs.push({ id: 'graph', kind: 'graph' });
  ns.value.active = 'graph';
}
export function closeTab(id) {
  const s = ns.value;
  const i = s.tabs.findIndex((t) => t.id === id);
  if (i < 0 || id === 'chat') return;
  s.tabs.splice(i, 1);
  if (s.active === id) s.active = s.tabs[Math.min(i, s.tabs.length - 1)].id;
}
export const activeTab = computed(() => ns.value?.tabs.find((t) => t.id === ns.value.active) ?? ns.value?.tabs[0]);

// Keep open tabs valid after renames/deletes.
function fixTabs(renames = []) {
  const valid = new Set();
  (function walk(nodes) { for (const n of nodes) n.type === 'dir' ? walk(n.children) : valid.add(n.path); })(store.tree);
  for (const s of Object.values(store.nb)) {
    for (const [from, to] of renames) {
      for (const t of s.tabs) if (t.path === from) { t.path = to; t.id = to; }
      if (s.active === from) s.active = to;
      if (s.topic === from) s.topic = to;
    }
    s.tabs = s.tabs.filter((t) => t.kind !== 'note' || valid.has(t.path));
    if (!s.tabs.some((t) => t.id === s.active)) s.active = 'chat';
  }
}

// Vault files (images, audio): the asset protocol in the desktop app, the file route in a browser.
export const fileUrl = (p) => (isDesktop && store.info.vault ? convertFileSrc(`${store.info.vault}/${p}`) : `/api/file?path=${encodeURIComponent(p)}`);

// action: optional { label, run } shown as a button (e.g. Undo).
export function toast(msg, kind = 'info', action = null) {
  const t = { id: Math.random(), msg, kind, action };
  store.toasts.push(t);
  setTimeout(() => (store.toasts = store.toasts.filter((x) => x.id !== t.id)), action ? 7000 : 4000);
}
export const dismissToast = (id) => (store.toasts = store.toasts.filter((x) => x.id !== id));

// ---- loading + persistence
let saveTimer, loadedVault = null;
// Only the tab you're using saves state, so a background tab (or a second window) can't
// overwrite it with stale data. A tab re-reads the saved state when you return to it.
const isActiveTab = () => document.visibilityState === 'visible' && document.hasFocus();
let dirtyWhileInactive = false;
function persist() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    if (!store.ready) return;
    if (!isActiveTab()) { dirtyWhileInactive = true; return; }
    dirtyWhileInactive = false;
    const nb = {};
    for (const [k, s] of Object.entries(store.nb)) {
      nb[k] = { ...s, chats: s.chats.map((c) => ({ ...c, messages: c.messages.slice(-50).map((m) => ({ ...m, sources: m.sources.map(({ text, ...r }) => ({ ...r, text: text?.slice(0, 300) })) })) })) };
    }
    api('/api/ui-state', { notebook: store.notebook, nb, studioOpen: store.studioOpen }, 'PUT').catch(() => {});
  }, 800);
}
watch(() => [store.nb, store.notebook, store.studioOpen], persist, { deep: true });
window.addEventListener('focus', async () => {
  if (!store.ready || dirtyWhileInactive) return persist();
  const saved = await api('/api/ui-state').catch(() => null);
  if (!saved?.nb) return;
  store.ready = false;
  store.nb = Object.fromEntries(Object.entries(saved.nb).map(([k, v]) => [k, migrate(v)]));
  if (saved.notebook) store.notebook = saved.notebook;
  if (saved.studioOpen != null) store.studioOpen = saved.studioOpen;
  validateSelection(); fixTabs();
  setTimeout(() => (store.ready = true), 0);
});

export async function loadAll() {
  // No vault yet (first launch): show the welcome screen instead of failing.
  const first = await api('/api/info');
  if (!first.vault) { store.info = first; store.ready = true; return; }
  const [info, notebooks, tree, palette, vaults, layout, templates] = await Promise.all([
    api('/api/info'), api('/api/notebooks'), api('/api/tree'), api('/api/palette'), api('/api/vaults'), api('/api/layout'), api('/api/templates').catch(() => []),
  ]);
  Object.assign(store, { info, notebooks, tree, palette, vaults, layout, templates });
  if (loadedVault !== info.vault) {
    store.ready = false;
    const saved = await api('/api/ui-state').catch(() => ({}));
    store.nb = Object.fromEntries(Object.entries(saved.nb || {}).map(([k, v]) => [k, migrate(v)]));
    store.notebook = saved.notebook || null;
    if (saved.studioOpen != null) store.studioOpen = saved.studioOpen;
    loadedVault = info.vault;
  }
  validateSelection();
  fixTabs();
  store.ready = true;
}

function validateSelection() {
  if (!store.notebooks.some((n) => n.path === store.notebook)) store.notebook = store.notebooks[0]?.path ?? null;
  for (const [path, s] of Object.entries(store.nb)) {
    const nb = store.notebooks.find((n) => n.path === path);
    if (!nb) { delete store.nb[path]; continue; }
    if (s.section && !nb.sections.some((x) => x.path === s.section)) s.section = null;
    if (s.topic && !allTopics(nb).some((t) => t.path === s.topic)) s.topic = null;
  }
}

export async function refreshFiles(renames) {
  [store.tree, store.palette, store.notebooks] = await Promise.all([api('/api/tree'), api('/api/palette'), api('/api/notebooks')]);
  validateSelection();
  fixTabs(renames);
}

// ---- live updates from the server (SSE)
const fileListeners = new Set();
export const onFileEvent = (fn) => (fileListeners.add(fn), () => fileListeners.delete(fn));
let treeTimer, unlisten = [];
const handlers = {
  index: (d) => { const was = store.index.running; store.index = d; if (was && !store.index.running) refreshFiles(); },
  overview: (d) => (store.overview = d.running),
  tree: () => { clearTimeout(treeTimer); treeTimer = setTimeout(() => refreshFiles(), 300); },
  file: (d) => fileListeners.forEach((fn) => fn(d)),
  models: async () => (store.info = await api('/api/info')),
  job: (j) => {
    if (j.error) toast(`${j.label}: ${j.error}`, 'error');
    if (j.finished) { if (store.jobs[j.id] && document.hidden) notify('Vault-LM', `${j.label.replace(/^(OCR|Downloading): /, '')} is ready`); delete store.jobs[j.id]; }
    else store.jobs[j.id] = j;
  },
  progress: (d) => (store.dueTotal = d.dueTotal),
};
// Live updates: Tauri events in the desktop app, Server-Sent Events in a browser.
export async function connectEvents() {
  unlisten.forEach((u) => u?.close ? u.close() : u?.());
  unlisten = [];
  if (isDesktop) {
    const { listen } = await import('@tauri-apps/api/event');
    for (const [name, fn] of Object.entries(handlers)) unlisten.push(await listen(name, (e) => fn(e.payload ?? {})));
    return;
  }
  const es = new EventSource('/api/events');
  for (const [name, fn] of Object.entries(handlers)) es.addEventListener(name, (e) => fn(JSON.parse(e.data)));
  unlisten.push(es);
}

export function resolveNote(name) {
  const lower = name.toLowerCase();
  const hit = store.palette.find((p) => !p.heading && (p.note.toLowerCase() === lower || p.path.replace(/\.md$/, '').toLowerCase() === lower));
  return hit?.path ?? null;
}

// Optional: Obsidian can open the same vault (plain URI, no integration needed).
export function openInObsidian(path) {
  openExternal(`obsidian://open?path=${encodeURIComponent(store.info.vault + '/' + path)}`);
}

export const MODES = [
  { id: 'reference', label: 'Reference', hint: 'Answers grounded in the whole notebook. Your selection comes first.' },
  { id: 'focused', label: 'Focused', hint: 'Stays on your selection, with a short notebook overview for context. Fewer stray facts.' },
];

export function toggleDark() {
  store.dark = !store.dark;
  document.documentElement.classList.toggle('dark', store.dark);
  localStorage.theme = store.dark ? 'dark' : 'light';
}

// OS notification (desktop only), e.g. when a background job finishes while the app isn't focused.
export async function notify(title, body) {
  if (!isDesktop) return;
  const n = await import('@tauri-apps/plugin-notification');
  if (!(await n.isPermissionGranted()) && (await n.requestPermission()) !== 'granted') return;
  n.sendNotification({ title, body });
}

export async function refreshDue() {
  const s = await api('/api/progress/summary').catch(() => null);
  if (s) store.dueTotal = s.dueTotal;
}
