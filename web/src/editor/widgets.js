import { WidgetType } from '@codemirror/view';

const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');

export class WikiLinkWidget extends WidgetType {
  constructor(target, heading, alias, resolved) { super(); Object.assign(this, { target, heading, alias, resolved }); }
  eq(o) { return o.target === this.target && o.heading === this.heading && o.alias === this.alias && o.resolved === this.resolved; }
  toDOM() {
    const a = document.createElement('a');
    a.className = `cm-wikilink${this.resolved ? '' : ' unresolved'}`;
    a.dataset.target = this.target;
    a.dataset.heading = this.heading || '';
    a.textContent = this.alias || (this.heading ? `${this.target ? this.target + ' › ' : ''}${this.heading}` : this.target);
    a.title = this.resolved ? 'Open' : 'Not found — click to create it';
    return a;
  }
  ignoreEvent() { return false; }
}

export class ImageWidget extends WidgetType {
  constructor(src, alt) { super(); this.src = src; this.alt = alt; }
  eq(o) { return o.src === this.src; }
  toDOM() {
    const img = document.createElement('img');
    img.src = this.src;
    img.alt = this.alt || '';
    img.className = 'cm-image';
    img.onerror = () => img.classList.add('broken');
    return img;
  }
}

export class CheckboxWidget extends WidgetType {
  constructor(checked, pos) { super(); this.checked = checked; this.pos = pos; }
  eq(o) { return o.checked === this.checked && o.pos === this.pos; }
  toDOM(view) {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.checked = this.checked;
    box.className = 'cm-task';
    box.onmousedown = (e) => e.preventDefault();
    box.onclick = () => view.dispatch({ changes: { from: this.pos + 1, to: this.pos + 2, insert: this.checked ? ' ' : 'x' } });
    return box;
  }
  ignoreEvent() { return true; }
}

const CALLOUT_ICONS = { note: '✎', info: 'ℹ', tip: '💡', hint: '💡', important: '❗', success: '✓', check: '✓', question: '?', warning: '⚠', caution: '⚠', danger: '⛔', error: '⛔', bug: '🐞', example: '◆', quote: '❝', abstract: '☰', summary: '☰', todo: '☐' };
export class CalloutWidget extends WidgetType {
  constructor(kind, hasTitle) { super(); this.kind = kind; this.hasTitle = hasTitle; }
  eq(o) { return o.kind === this.kind && o.hasTitle === this.hasTitle; }
  toDOM() {
    const s = document.createElement('span');
    s.className = 'cm-callout-label';
    s.textContent = `${CALLOUT_ICONS[this.kind] || '✎'} ${this.hasTitle ? '' : this.kind[0].toUpperCase() + this.kind.slice(1)}`.trimEnd() + ' ';
    return s;
  }
}

export class BulletWidget extends WidgetType {
  eq() { return true; }
  toDOM() { const s = document.createElement('span'); s.className = 'cm-bullet'; s.textContent = '•'; return s; }
}

export class HrWidget extends WidgetType {
  eq() { return true; }
  toDOM() { const s = document.createElement('span'); s.className = 'cm-hr'; return s; }
}

// Collapsed YAML frontmatter shown as a Properties panel; click to edit the raw YAML.
export class PropertiesWidget extends WidgetType {
  constructor(yaml) { super(); this.yaml = yaml; }
  eq(o) { return o.yaml === this.yaml; }
  toDOM() {
    const wrap = document.createElement('div');
    wrap.className = 'cm-properties';
    const rows = this.yaml.split('\n').filter((l) => l.trim()).map((l) => {
      const m = l.match(/^([\w-]+):\s*(.*)$/);
      return m ? `<div class="cm-prop"><span class="k">${esc(m[1])}</span><span class="v">${esc(m[2]) || '<i>empty</i>'}</span></div>`
        : `<div class="cm-prop"><span class="k"></span><span class="v">${esc(l.trim())}</span></div>`;
    }).join('');
    wrap.innerHTML = `<div class="cm-prop-title">Properties</div>${rows}`;
    return wrap;
  }
  ignoreEvent() { return false; }
}
