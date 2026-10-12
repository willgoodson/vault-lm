<script setup>
import { ref, reactive, computed } from 'vue';
import { BookOpen, Hash, FileText, Paperclip, Plus, FolderPlus, FilePlus, MoreHorizontal, Pencil, Trash2, ExternalLink, Info, FolderInput, ChevronDown, LayoutTemplate } from 'lucide-vue-next';
import DetailsDialog from './DetailsDialog.vue';
import { store, ns, selectNotebook, select, openTab, refreshFiles, toast, openInObsidian } from '../store.js';
import { api } from '../api.js';
import TreeRow from './TreeRow.vue';
import InlineInput from './InlineInput.vue';
import Popover from './Popover.vue';
import AddSourcePopover from './AddSourcePopover.vue';

const expanded = reactive(JSON.parse(localStorage.vlmExpanded || '{}'));
const toggle = (p) => { expanded[p] = !expanded[p]; localStorage.vlmExpanded = JSON.stringify(expanded); };
const open = (p) => { if (!expanded[p]) toggle(p); };

const creating = ref(null);   // { parent, kind: 'notebook'|'section'|'topic' }
const renaming = ref(null);   // path
const menu = reactive({ open: false, anchor: null, item: null });
const source = reactive({ open: false, anchor: null, target: null });
const details = ref(null);
const props = defineProps({ hideDone: Boolean });
const shown = (list) => (props.hideDone ? list.filter((x) => x.meta?.status !== 'done') : list);

// ---- selection: notebook/section/topic rows set the scope
const st = (nb) => store.nb[nb.path];
function clickNotebook(nb) {
  if (store.notebook !== nb.path) selectNotebook(nb.path); // switching restores that notebook's own selection + tabs
  else select({});                                         // clicking the current notebook selects all of it
  open(nb.path);
}
function clickSection(nb, s) { selectNotebook(nb.path); select({ section: s.path }); open(s.path); }
function clickTopic(nb, s, t) { selectNotebook(nb.path); select({ section: s?.path || null, topic: t.path }); openTab(t.path); }
const openSource = (nb, src) => { selectNotebook(nb.path); openTab(src.path); };

const stateOf = (nb, s, t) => {
  if (store.notebook !== nb.path) return undefined;
  const sel = ns.value;
  if (t) return sel.topic === t.path ? 'on' : undefined;
  if (s) return sel.section === s.path ? (sel.topic ? 'parent' : 'on') : undefined;
  return sel.section || sel.topic ? 'parent' : 'on';
};

// ---- create / rename / delete
const tplMenu = reactive({ open: false, anchor: null, parent: null });
const showTemplates = (e, parent) => Object.assign(tplMenu, { open: true, anchor: e.currentTarget, parent });
const tplName = (id) => store.templates.find((t) => t.id === id)?.name;
function startCreate(parent, kind, template = null) {
  tplMenu.open = false;
  creating.value = { parent, kind, template };
  if (parent) open(parent);
  menu.open = false;
}
async function commitCreate(name) {
  const { parent, kind, template } = creating.value;
  creating.value = null;
  try {
    if (kind === 'topic') {
      const { path } = await api('/api/topic', { folder: parent, name, template });
      await refreshFiles();
      const nb = store.notebooks.find((n) => path.startsWith(n.path + '/'));
      const sec = nb?.sections.find((s) => path.startsWith(s.path + '/'));
      if (nb) { selectNotebook(nb.path); select({ section: sec?.path || null, topic: path }); openTab(path); }
    } else {
      const { path } = await api('/api/folder', { parent, name });
      await refreshFiles();
      if (kind === 'notebook') { selectNotebook(path); open(path); }
      else { selectNotebook(parent); select({ section: path }); }
    }
  } catch (e) { toast(e.message, 'error'); }
}

// After a server-side rename/move: re-key per-notebook UI state and refresh.
async function afterRelocate(from, r) {
  for (const [k, st] of Object.entries(store.nb)) {
    const fix = (p) => (p && (p === from || p.startsWith(from + '/')) ? r.path + p.slice(from.length) : p);
    st.section = fix(st.section); st.topic = fix(st.topic);
    if (k === from || k.startsWith(from + '/')) { store.nb[fix(k)] = st; delete store.nb[k]; }
  }
  if (store.notebook === from) store.notebook = r.path;
  await refreshFiles(r.moved);
}

async function commitRename(item, name) {
  renaming.value = null;
  try {
    const r = await api('/api/rename', { path: item.path, name });
    await afterRelocate(item.path, r);
    if (r.updated.length) toast(`Renamed, and updated links in ${r.updated.length} note${r.updated.length > 1 ? 's' : ''}`, 'success');
  } catch (e) { toast(e.message, 'error'); }
}

const parentOf = (p) => p.split('/').slice(0, -1).join('/');
async function moveItem(item, toFolder, { undo = true } = {}) {
  const from = item.path;
  const oldParent = parentOf(from);
  if (toFolder === oldParent) return;
  try {
    const r = await api('/api/move', { path: from, toFolder });
    await afterRelocate(from, r);
    const where = toFolder.split('/').pop() || 'the vault root';
    toast(`Moved “${item.name}” to ${where}${r.updated.length ? ` · ${r.updated.length} link${r.updated.length > 1 ? 's' : ''} updated` : ''}`, 'success',
      undo ? { label: 'Undo', run: () => moveItem({ ...item, path: r.path }, oldParent, { undo: false }) } : null);
  } catch (e) { toast(e.message, 'error'); }
}

// ---- drag & drop: topics → sections/notebooks, sections → notebooks
const drag = ref(null);      // { kind, path, name }
const over = ref(null);      // path of the highlighted drop target
const canDrop = (target) => {
  const d = drag.value;
  if (!d || target.path === d.path || parentOf(d.path) === target.path) return false;
  if (d.kind === 'topic') return target.kind === 'notebook' || target.kind === 'section';
  if (d.kind === 'section') return target.kind === 'notebook';
  return false;
};
function onDragStart(e, item) { drag.value = item; e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', item.path); }
function onDragOver(e, target) { if (!canDrop(target)) { e.dataTransfer.dropEffect = 'none'; return; } e.preventDefault(); e.dataTransfer.dropEffect = 'move'; over.value = target.path; }
function onDrop(e, target) {
  e.preventDefault();
  const d = drag.value, ok = canDrop(target); // check before clearing drag state
  over.value = null; drag.value = null;
  if (d && ok) moveItem(d, target.path);
}
function onDragEnd() { drag.value = null; over.value = null; }
const dnd = (item, asTarget = true) => ({
  draggable: item.kind !== 'notebook',
  onDragstart: (e) => onDragStart(e, item),
  onDragend: onDragEnd,
  ...(asTarget && { onDragover: (e) => onDragOver(e, item), onDragleave: () => over.value === item.path && (over.value = null), onDrop: (e) => onDrop(e, item) }),
  class: over.value === item.path ? 'ring-2 ring-accent-500/60 bg-accent-500/10' : drag.value?.path === item.path ? 'opacity-40' : '',
});

// ---- "Move to…" picker
const mover = reactive({ open: false, anchor: null, item: null });
const destinations = computed(() => {
  const it = mover.item;
  if (!it) return [];
  const out = [];
  for (const nb of store.notebooks) {
    out.push({ path: nb.path, label: nb.name, kind: 'notebook', depth: 0 });
    if (it.kind === 'topic') for (const s of nb.sections) out.push({ path: s.path, label: s.name, kind: 'section', depth: 1 });
  }
  return out.filter((d) => d.path !== parentOf(it.path) && d.path !== it.path);
});

async function removeSource(src) {
  menu.open = false;
  if (!confirm(`Remove the source “${src.name}”? It moves to the vault trash (.trash/), along with its original file.`)) return;
  try { await api('/api/source/remove', { path: src.path }); await refreshFiles(); toast(`Removed “${src.name}”`, 'success'); }
  catch (e) { toast(e.message, 'error'); }
}

async function remove(item) {
  if (item.kind === 'source') return removeSource(item);
  menu.open = false;
  const what = item.kind === 'topic' ? 'topic' : item.kind;
  if (!confirm(`Move ${what} “${item.name}” to the vault trash (.trash/)?`)) return;
  try { await api('/api/trash', { path: item.path }); await refreshFiles(); toast(`Moved “${item.name}” to .trash`, 'success'); }
  catch (e) { toast(e.message, 'error'); }
}

function showMenu(e, item) { Object.assign(menu, { open: true, anchor: e.currentTarget, item }); }
function showSource(e, target) { Object.assign(source, { open: true, anchor: e.currentTarget, target }); }
const act = 'grid size-6 place-items-center rounded-md text-zinc-400 transition hover:bg-zinc-200 hover:text-zinc-800 dark:hover:bg-zinc-700 dark:hover:text-zinc-100';

defineExpose({ startCreate });
</script>

<template>
  <div class="flex flex-col gap-px">
    <template v-for="nb in shown(store.notebooks)" :key="nb.path">
      <!-- Notebook -->
      <TreeRow v-bind="dnd({ ...nb, kind: 'notebook' })" :label="nb.name" :icon="BookOpen" :state="stateOf(nb)" :meta="nb.meta" :expandable="true" :expanded="!!expanded[nb.path]"
        :count="nb.sections.length + nb.topics.length || ''" :renaming="renaming === nb.path"
        @select="clickNotebook(nb)" @toggle="toggle(nb.path)" @rename="(v) => commitRename({ ...nb, kind: 'notebook' }, v)" @cancel-rename="renaming = null">
        <template #actions>
          <button :class="act" title="New section" @click="startCreate(nb.path, 'section')"><FolderPlus class="size-3.5" /></button>
          <button :class="act" title="New topic" @click="startCreate(nb.path, 'topic')"><FilePlus class="size-3.5" /></button>
          <button :class="act" class="!w-3.5" title="New topic from a template" @click="showTemplates($event, nb.path)"><ChevronDown class="size-3" /></button>
          <button :class="act" title="Add source" @click="showSource($event, { notebook: nb.path })"><Paperclip class="size-3.5" /></button>
          <button :class="act" title="More" @click="showMenu($event, { ...nb, kind: 'notebook' })"><MoreHorizontal class="size-3.5" /></button>
        </template>
      </TreeRow>

      <template v-if="expanded[nb.path]">
        <!-- Notebook-level sources -->
        <template v-if="nb.sources.length">
          <TreeRow :depth="1" :label="`Sources`" :icon="Paperclip" muted :expandable="true" :expanded="!!expanded[nb.path + '#src']" :count="nb.sources.length"
            @select="toggle(nb.path + '#src')" @toggle="toggle(nb.path + '#src')" />
          <template v-if="expanded[nb.path + '#src']">
            <TreeRow v-for="src in nb.sources" :key="src.path" :depth="2" :label="src.name" :icon="Paperclip" muted :renaming="renaming === src.path" @select="openSource(nb, src)" @rename="(v) => commitRename({ ...src, kind: 'source' }, v)" @cancel-rename="renaming = null">
              <template #actions>
                <button :class="act" title="Remove source" @click="removeSource(src)"><Trash2 class="size-3.5" /></button>
                <button :class="act" title="More" @click="showMenu($event, { ...src, kind: 'source' })"><MoreHorizontal class="size-3.5" /></button>
              </template>
            </TreeRow>
          </template>
        </template>

        <!-- Sections -->
        <template v-for="s in shown(nb.sections)" :key="s.path">
          <TreeRow v-bind="dnd({ ...s, kind: 'section' })" :depth="1" :label="s.name" :icon="Hash" :state="stateOf(nb, s)" :meta="s.meta" :expandable="!!(s.topics.length || s.sources.length) || creating?.parent === s.path"
            :expanded="!!expanded[s.path]" :count="s.topics.length || ''" :renaming="renaming === s.path"
            @select="clickSection(nb, s)" @toggle="toggle(s.path)" @rename="(v) => commitRename({ ...s, kind: 'section' }, v)" @cancel-rename="renaming = null">
            <template #actions>
              <button :class="act" title="New topic" @click="startCreate(s.path, 'topic')"><FilePlus class="size-3.5" /></button>
              <button :class="act" class="!w-3.5" title="New topic from a template" @click="showTemplates($event, s.path)"><ChevronDown class="size-3" /></button>
              <button :class="act" title="Add source" @click="showSource($event, { notebook: nb.path, section: s.path })"><Paperclip class="size-3.5" /></button>
              <button :class="act" title="More" @click="showMenu($event, { ...s, kind: 'section' })"><MoreHorizontal class="size-3.5" /></button>
            </template>
          </TreeRow>
          <template v-if="expanded[s.path]">
            <template v-if="s.sources.length">
              <TreeRow :depth="2" label="Sources" :icon="Paperclip" muted :expandable="true" :expanded="!!expanded[s.path + '#src']" :count="s.sources.length"
                @select="toggle(s.path + '#src')" @toggle="toggle(s.path + '#src')" />
              <template v-if="expanded[s.path + '#src']">
                <TreeRow v-for="src in s.sources" :key="src.path" :depth="3" :label="src.name" :icon="Paperclip" muted :renaming="renaming === src.path" @select="openSource(nb, src)" @rename="(v) => commitRename({ ...src, kind: 'source' }, v)" @cancel-rename="renaming = null">
              <template #actions>
                <button :class="act" title="Remove source" @click="removeSource(src)"><Trash2 class="size-3.5" /></button>
                <button :class="act" title="More" @click="showMenu($event, { ...src, kind: 'source' })"><MoreHorizontal class="size-3.5" /></button>
              </template>
            </TreeRow>
              </template>
            </template>
            <template v-for="t in s.topics" :key="t.path">
              <TreeRow v-bind="dnd({ ...t, kind: 'topic' }, false)" :depth="2" :label="t.name" :icon="FileText" :state="stateOf(nb, s, t)" :expandable="t.sources.length > 0" :expanded="!!expanded[t.path]"
                :clips="t.sources.length" :renaming="renaming === t.path"
                @select="clickTopic(nb, s, t)" @toggle="toggle(t.path)" @rename="(v) => commitRename({ ...t, kind: 'topic' }, v)" @cancel-rename="renaming = null">
                <template #actions>
                  <button :class="act" title="Add source to this topic" @click="showSource($event, { notebook: nb.path, section: s.path, topic: t.path })"><Paperclip class="size-3.5" /></button>
                  <button :class="act" title="More" @click="showMenu($event, { ...t, kind: 'topic' })"><MoreHorizontal class="size-3.5" /></button>
                </template>
              </TreeRow>
              <template v-if="expanded[t.path]">
                <TreeRow v-for="src in t.sources" :key="src.path" :depth="3" :label="src.name" :icon="Paperclip" muted :renaming="renaming === src.path" @select="openSource(nb, src)" @rename="(v) => commitRename({ ...src, kind: 'source' }, v)" @cancel-rename="renaming = null">
              <template #actions>
                <button :class="act" title="Remove source" @click="removeSource(src)"><Trash2 class="size-3.5" /></button>
                <button :class="act" title="More" @click="showMenu($event, { ...src, kind: 'source' })"><MoreHorizontal class="size-3.5" /></button>
              </template>
            </TreeRow>
              </template>
            </template>
            <div v-if="creating?.parent === s.path" class="py-0.5 pr-2" :style="{ paddingLeft: '50px' }">
              <InlineInput :placeholder="creating.template ? `Topic name · ${tplName(creating.template)}` : 'Topic name'" @commit="commitCreate" @cancel="creating = null" />
            </div>
          </template>
        </template>
        <div v-if="creating?.parent === nb.path && creating.kind === 'section'" class="py-0.5 pr-2" style="padding-left: 36px">
          <InlineInput placeholder="Section name" @commit="commitCreate" @cancel="creating = null" />
        </div>

        <!-- Topics directly in the notebook -->
        <template v-for="t in nb.topics" :key="t.path">
          <TreeRow v-bind="dnd({ ...t, kind: 'topic' }, false)" :depth="1" :label="t.name" :icon="FileText" :state="stateOf(nb, null, t)" :expandable="t.sources.length > 0" :expanded="!!expanded[t.path]"
            :clips="t.sources.length" :renaming="renaming === t.path"
            @select="clickTopic(nb, null, t)" @toggle="toggle(t.path)" @rename="(v) => commitRename({ ...t, kind: 'topic' }, v)" @cancel-rename="renaming = null">
            <template #actions>
              <button :class="act" title="Add source to this topic" @click="showSource($event, { notebook: nb.path, topic: t.path })"><Paperclip class="size-3.5" /></button>
              <button :class="act" title="More" @click="showMenu($event, { ...t, kind: 'topic' })"><MoreHorizontal class="size-3.5" /></button>
            </template>
          </TreeRow>
          <template v-if="expanded[t.path]">
            <TreeRow v-for="src in t.sources" :key="src.path" :depth="2" :label="src.name" :icon="Paperclip" muted :renaming="renaming === src.path" @select="openSource(nb, src)" @rename="(v) => commitRename({ ...src, kind: 'source' }, v)" @cancel-rename="renaming = null">
              <template #actions>
                <button :class="act" title="Remove source" @click="removeSource(src)"><Trash2 class="size-3.5" /></button>
                <button :class="act" title="More" @click="showMenu($event, { ...src, kind: 'source' })"><MoreHorizontal class="size-3.5" /></button>
              </template>
            </TreeRow>
          </template>
        </template>
        <div v-if="creating?.parent === nb.path && creating.kind === 'topic'" class="py-0.5 pr-2" style="padding-left: 36px">
          <InlineInput :placeholder="creating.template ? `Topic name · ${tplName(creating.template)}` : 'Topic name'" @commit="commitCreate" @cancel="creating = null" />
        </div>
      </template>
    </template>

    <div v-if="creating?.kind === 'notebook'" class="py-0.5 pr-2 pl-6">
      <InlineInput placeholder="Notebook name" @commit="commitCreate" @cancel="creating = null" />
    </div>
    <button v-if="!store.notebooks.length && !creating" class="mx-1 mt-1 rounded-lg border border-dashed border-zinc-300 px-3 py-3 text-left text-xs text-zinc-500 hover:border-accent-400 dark:border-zinc-700" @click="startCreate(store.layout.root, 'notebook')">
      <Plus class="mb-1 size-4" /> Create your first notebook
    </button>

    <Popover :anchor="menu.anchor" :open="menu.open" :width="200" align="end" @close="menu.open = false">
      <template v-if="menu.item">
        <button v-if="menu.item.kind === 'notebook'" class="menu-item" @click="startCreate(menu.item.path, 'section')"><FolderPlus class="size-3.5" /> New section</button>
        <button v-if="menu.item.kind === 'notebook' || menu.item.kind === 'section'" class="menu-item" @click="startCreate(menu.item.path, 'topic')"><FilePlus class="size-3.5" /> New topic</button>
        <button v-if="menu.item.kind === 'notebook' || menu.item.kind === 'section'" class="menu-item" @click="details = menu.item; menu.open = false"><Info class="size-3.5" /> Edit details…</button>
        <button class="menu-item" @click="renaming = menu.item.path; menu.open = false"><Pencil class="size-3.5" /> Rename</button>
        <button v-if="menu.item.kind === 'topic' || menu.item.kind === 'section'" class="menu-item" @click="Object.assign(mover, { open: true, anchor: menu.anchor, item: menu.item }); menu.open = false"><FolderInput class="size-3.5" /> Move to…</button>
        <button class="menu-item" @click="openInObsidian(menu.item.path); menu.open = false"><ExternalLink class="size-3.5" /> Open in Obsidian</button>
        <div class="my-1 h-px bg-zinc-100 dark:bg-zinc-800" />
        <button class="menu-item !text-red-600 hover:!bg-red-500/10" @click="remove(menu.item)"><Trash2 class="size-3.5" /> {{ menu.item.kind === 'source' ? 'Remove source' : 'Move to trash' }}</button>
      </template>
    </Popover>
    <DetailsDialog :item="details" @close="details = null" />
    <Popover :anchor="tplMenu.anchor" :open="tplMenu.open" :width="220" align="end" @close="tplMenu.open = false">
      <div class="px-2 pt-1 pb-1.5 text-xs text-zinc-500">New topic from…</div>
      <button class="menu-item" @click="startCreate(tplMenu.parent, 'topic')"><FilePlus class="size-3.5" /> Blank note</button>
      <button v-for="t in store.templates" :key="t.id" class="menu-item" @click="startCreate(tplMenu.parent, 'topic', t.id)"><LayoutTemplate class="size-3.5" /> {{ t.name }}</button>
    </Popover>
    <Popover :anchor="mover.anchor" :open="mover.open" :width="240" align="end" @close="mover.open = false">
      <div class="px-2 pt-1 pb-1.5 text-xs text-zinc-500">Move <b class="text-zinc-800 dark:text-zinc-100">{{ mover.item?.name }}</b> to…</div>
      <div class="max-h-72 overflow-y-auto">
        <button v-for="d in destinations" :key="d.path" class="menu-item" :style="{ paddingLeft: `${8 + d.depth * 16}px` }" @click="moveItem(mover.item, d.path); mover.open = false">
          <component :is="d.kind === 'notebook' ? BookOpen : Hash" class="size-3.5 opacity-60" /> {{ d.label }}
        </button>
      </div>
    </Popover>
    <AddSourcePopover :anchor="source.anchor" :open="source.open" :target="source.target" @close="source.open = false" />
  </div>
</template>
