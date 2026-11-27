/* Mindloom — inspector, outline, minimap, status bar, dialogs */
(function (ML) {
  'use strict';
  const App = ML.app, M = ML.model, T = ML.themes, R = ML.render, Lay = ML.layout, IO = ML.io, P = ML.platform;
  const $ = (s) => document.querySelector(s);
  const esc = R.esc;

  // ================= modals =================
  App.modal = function ({ title, body, foot, cls, onMount, onClose }) {
    const root = $('#modal-root');
    const ov = document.createElement('div');
    ov.className = 'overlay';
    ov.innerHTML = `<div class="modal ${cls || ''}"><div class="modal-head"><h2>${title}</h2><button class="icon-btn" data-close>✕</button></div><div class="modal-body">${body}</div>${foot ? `<div class="modal-foot">${foot}</div>` : ''}</div>`;
    root.appendChild(ov);
    let closed = false;
    const close = (v) => {
      if (closed) return;
      closed = true;
      ov.remove();
      App.listeners['escape-modal'] = (App.listeners['escape-modal'] || []).filter((f) => f !== onEsc);
      if (onClose) onClose(v);
      document.getElementById('stage').focus({ preventScroll: true });
    };
    const onEsc = () => { if (root.lastElementChild === ov) close(null); };
    App.on('escape-modal', onEsc);
    ov.addEventListener('pointerdown', (e) => { if (e.target === ov) close(null); });
    ov.querySelector('[data-close]').onclick = () => close(null);
    if (onMount) onMount(ov.querySelector('.modal'), close);
    return close;
  };

  App.ask = (title, msg, buttons) => new Promise((resolve) => {
    App.modal({
      title, cls: 'small', body: `<p>${esc(msg)}</p>`,
      foot: buttons.map((b, i) => `<button class="btn ${b.cls || ''}" data-i="${i}">${b.label}</button>`).join(''),
      onMount(m, close) {
        m.querySelectorAll('[data-i]').forEach((b) => { b.onclick = () => close(buttons[+b.dataset.i].value); });
        const prim = m.querySelector('.btn.primary');
        if (prim) prim.focus();
      },
      onClose: (v) => resolve(v == null ? 'cancel' : v),
    });
  });

  App.prompt = (title, label, value, placeholder) => new Promise((resolve) => {
    let out = null;
    App.modal({
      title, cls: 'small',
      body: `<label class="muted">${label}</label><input class="field" id="pr-in" value="${esc(value || '')}" placeholder="${esc(placeholder || '')}" style="margin-top:6px"/>`,
      foot: '<button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>OK</button>',
      onMount(m, close) {
        const inp = m.querySelector('#pr-in');
        inp.focus(); inp.select();
        const ok = () => { out = inp.value; close(true); };
        m.querySelector('[data-ok]').onclick = ok;
        m.querySelector('[data-x]').onclick = () => close(null);
        inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); ok(); } });
      },
      onClose: () => resolve(out),
    });
  });

  // ================= mini previews (real renderer, tiny sample maps) =================
  const previewCache = new Map();
  function sampleDoc() {
    const d = M.createDoc('Idea');
    const a = M.addChild(d, d.root.id, 'Plan');
    const b = M.addChild(d, d.root.id, 'Build');
    const c = M.addChild(d, d.root.id, 'Share');
    M.addChild(d, a.id, 'Goals'); M.addChild(d, b.id, 'Code'); M.addChild(d, b.id, 'Test'); M.addChild(d, c.id, 'Blog');
    const e = M.addChild(d, d.root.id, 'Learn');
    M.addChild(d, e.id, 'Books');
    return d;
  }
  function preview(settingsPatch, key) {
    if (previewCache.has(key)) return previewCache.get(key);
    const d = sampleDoc();
    Object.assign(d.settings, settingsPatch);
    d.settings.shapes = Object.assign({}, App.doc.settings.shapes, settingsPatch.shapes || {});
    const theme = T.getTheme(d.settings.theme);
    const L = Lay.layout(d, { theme, settings: d.settings, measure: App.measure });
    const out = R.exportSVG(L, d, { theme, settings: d.settings, padding: 16 });
    const url = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(out.svg);
    previewCache.set(key, url);
    return url;
  }
  const img = (url, h) => `<img src="${url}" style="width:100%;height:${h}px;object-fit:contain;border-radius:6px;display:block" alt=""/>`;

  // ================= inspector =================
  let tab = 'topic';
  const insp = () => $('#insp-body');
  function showTab(t) {
    tab = t;
    document.body.classList.remove('no-inspector');
    document.querySelectorAll('#insp-tabs button').forEach((b) => b.classList.toggle('active', b.dataset.tab === t));
    renderInspector(true);
    requestAnimationFrame(() => App.applyView());
  }
  App.on('show-tab', showTab);

  function swatches(key, current, list) {
    list = list || T.SWATCHES;
    return `<div class="swatches" data-key="${key}">${list.map((c) => `<button class="sw${c === 'transparent' ? ' none' : ''}${current === c ? ' on' : ''}" data-val="${c}" title="${c}" style="background:${c}"></button>`).join('')}<button class="sw custom" title="Custom colour"><input type="color" data-key="${key}" value="${/^#[0-9a-f]{6}$/i.test(current || '') ? current : '#3b82f6'}"/></button></div>`;
  }
  const seg = (key, cur, opts) => `<div class="seg" data-seg="${key}">${opts.map(([v, l]) => `<button data-val="${v}" class="${String(cur) === String(v) ? 'on' : ''}">${l}</button>`).join('')}</div>`;
  const range = (key, label, val, min, max, step, unit) => `<div class="row"><label>${label}</label><input type="range" data-range="${key}" min="${min}" max="${max}" step="${step || 1}" value="${val}"/><span class="val">${val}${unit || ''}</span></div>`;

  function shapeIcon(id) {
    const w = 40, h = 22, s = 'stroke="currentColor" stroke-width="1.6" fill="none"';
    const shapes = {
      rounded: `<rect x="2" y="3" width="36" height="16" rx="5" ${s}/>`,
      rect: `<rect x="2" y="3" width="36" height="16" ${s}/>`,
      pill: `<rect x="2" y="3" width="36" height="16" rx="8" ${s}/>`,
      ellipse: `<ellipse cx="20" cy="11" rx="18" ry="9" ${s}/>`,
      circle: `<circle cx="20" cy="11" r="9.5" ${s}/>`,
      underline: `<path d="M3 17H37" ${s}/><path d="M9 11h22" stroke="currentColor" stroke-opacity=".4" stroke-width="3"/>`,
      diamond: `<polygon points="20,1 38,11 20,21 2,11" ${s}/>`,
      hexagon: `<polygon points="8,2 32,2 38,11 32,20 8,20 2,11" ${s}/>`,
      parallelogram: `<polygon points="8,3 38,3 32,19 2,19" ${s}/>`,
      cloud: `<path d="M10 18a5 5 0 0 1-1-9.8A7 7 0 0 1 22 5a6 6 0 0 1 10 4 4.5 4.5 0 0 1-1 9z" ${s}/>`,
      none: `<path d="M9 11h22" stroke="currentColor" stroke-opacity=".5" stroke-width="3"/>`,
    };
    return `<svg viewBox="0 0 ${w} ${h}" style="width:40px;height:22px">${shapes[id] || ''}</svg>`;
  }

  function topicTab() {
    if (App.selRel) {
      const r = App.doc.relations.find((x) => x.id === App.selRel);
      if (!r) return '<p class="muted">Nothing selected.</p>';
      return `<div class="sec"><h4>Relationship</h4>
        <div class="row"><input class="field" data-rel-label value="${esc(r.label || '')}" placeholder="Label (e.g. depends on)"/></div>
        <h4 style="margin-top:12px">Colour</h4>${swatches('relColor', r.color, ['#5c677d', '#e03131', '#f76707', '#2f9e44', '#1c7ed6', '#7048e8', '#c2255c', '#000000', '#ffffff'])}
        <div class="row" style="margin-top:12px"><button class="btn" data-cmd="relationReverse">⇄ Reverse</button><button class="btn danger" data-cmd="deleteSel">Delete</button></div></div>`;
    }
    const n = App.primaryNode();
    const t = n && App.L.nodes.get(n.id);
    if (!n || !t) {
      return `<div class="sec"><p class="muted">Select a topic to style it. Use the <b>Map</b> tab for themes, structures and backgrounds.</p></div>
        <div class="sec"><h4>Quick start</h4><p class="muted" style="line-height:1.7">
        <kbd>Tab</kbd> add subtopic<br/><kbd>Enter</kbd> add topic<br/><kbd>Space</kbd> / <kbd>F2</kbd> edit text<br/>Drag a topic anywhere — it stays connected<br/>Drop it onto another topic to re-parent it<br/>Double-click canvas → floating topic<br/>Right-click for more actions</p></div>`;
    }
    const st = t.box.style, ns = n.style || {};
    const f = M.find(App.doc, n.id);
    const isLvl1 = f.depth === 1 && !f.floating && App.doc.settings.layout === 'mindmap';
    return `
      <div class="sec"><h4>Shape <button class="btn" data-reset-style style="padding:1px 8px;font-size:10.5px;text-transform:none;letter-spacing:0">Reset style</button></h4>
        <div class="grid c4">${T.SHAPES.map((s) => `<div class="card${st.shape === s.id ? ' on' : ''}" data-shape="${s.id}" title="${s.name}">${shapeIcon(s.id)}<span class="nm">${s.name}</span></div>`).join('')}</div></div>
      <div class="sec"><h4>Fill</h4>${swatches('fill', ns.fill)}</div>
      <div class="sec"><h4>Text</h4>
        <div class="row"><div class="toggle-row">
          <button class="tbtn${st.bold ? ' on' : ''}" data-toggle="bold" title="Bold"><b>B</b></button>
          <button class="tbtn${st.italic ? ' on' : ''}" data-toggle="italic" title="Italic"><i>I</i></button>
          <button class="tbtn${st.underline ? ' on' : ''}" data-toggle="underline" title="Underline"><u>U</u></button>
          <button class="tbtn${st.strike ? ' on' : ''}" data-toggle="strike" title="Strikethrough"><s>S</s></button>
        </div>${seg('align', st.align, [['left', '⟸'], ['center', '≡'], ['right', '⟹']])}</div>
        ${range('fontSize', 'Size', st.fontSize, 9, 60, 1, 'px')}
        <div class="row"><label>Font</label><select class="field" data-sel="font"><option value="">Map default</option>${T.FONTS.map((x) => `<option value="${x.id}"${ns.font === x.id ? ' selected' : ''}>${x.name}</option>`).join('')}</select></div>
        ${range('width', 'Wrap width', ns.width || App.doc.settings.maxWidth + (f.depth === 0 && !f.floating ? 80 : 0), 80, 700, 10, '')}
        <h4 style="margin-top:10px">Text colour</h4>${swatches('color', ns.color, ['#000000', '#343a40', '#868e96', '#ffffff', '#e03131', '#f76707', '#f59f00', '#2f9e44', '#0c8599', '#1c7ed6', '#6741d9', '#c2255c'])}
      </div>
      <div class="sec"><h4>Border</h4>${swatches('border', ns.border)}
        <div class="row" style="margin-top:8px"><label>Width</label>${seg('borderWidth', ns.borderWidth != null ? ns.borderWidth : '', [['', 'Auto'], [0, '0'], [1, '1'], [2, '2'], [3, '3'], [4, '4']])}</div>
        <div class="row"><label>Style</label>${seg('borderDash', st.borderDash, [['solid', 'Solid'], ['dashed', 'Dashed'], ['dotted', 'Dotted']])}</div>
      </div>
      <div class="sec"><h4>Branch line colour</h4>${swatches('lineColor', ns.lineColor, ['#adb5bd', '#495057', '#e03131', '#f76707', '#f59f00', '#2f9e44', '#0c8599', '#1c7ed6', '#6741d9', '#c2255c'])}</div>
      <div class="sec"><h4>Task</h4>${seg('task', n.task || '', [['', 'None'], ['todo', '☐ To do'], ['done', '☑ Done']])}</div>
      <div class="sec"><h4>Due date</h4><div class="row"><input type="date" class="field" data-field="due" value="${esc(n.due || '')}"/>${n.due ? '<button class="btn" data-clear-due>Clear</button>' : ''}</div>${n.due && M.isOverdue(n) ? '<p class="muted" style="color:var(--ui-danger)">⚠ Overdue — the date chip on the map turns red until the task is done.</p>' : '<p class="muted">Shown as a 📅 chip on the topic.</p>'}</div>
      <div class="sec"><h4>Labels</h4><input class="field" data-field="tags" value="${esc((n.tags || []).join(', '))}" placeholder="Comma separated, e.g. urgent, Q3"/></div>
      <div class="sec"><h4>Hyperlink</h4><div class="row"><input class="field" data-field="link" value="${esc(n.link || '')}" placeholder="https://…"/>${n.link ? '<button class="btn" data-cmd="openLink">Open</button>' : ''}</div></div>
      <div class="sec"><h4>Image</h4>${n.image ? `${range('imgW', 'Width', n.image.w, 40, 600, 10, '')}<div class="row"><button class="btn" data-cmd="image">Replace…</button><button class="btn danger" data-remove-image>Remove</button></div>` : '<button class="btn" data-cmd="image">Insert image…</button> <span class="muted">or paste / drop one</span>'}</div>
      <div class="sec"><h4>Boundary</h4><div class="row">${seg('boundary', n.boundary ? 'on' : '', [['', 'Off'], ['on', 'On']])}</div>${n.boundary ? `<input class="field" data-field="boundaryLabel" value="${esc(n.boundaryLabel || '')}" placeholder="Boundary title (optional)"/>` : ''}</div>
      ${isLvl1 ? `<div class="sec"><h4>Side of the map</h4>${seg('side', n.side || '', [['', 'Auto'], ['left', 'Left'], ['right', 'Right']])}</div>` : ''}
    `;
  }

  function mapTab() {
    const s = App.doc.settings;
    const theme = App.theme();
    return `
      <div class="sec"><h4>Map style</h4><div class="grid c3">${T.MAP_STYLES.map((ms) => {
        const on = s.layout === ms.layout && s.lineStyle === ms.lineStyle && s.shapes.root === ms.shapes.root && s.shapes.main === ms.shapes.main && s.shapes.sub === ms.shapes.sub;
        return `<div class="card${on ? ' on' : ''}" data-mapstyle="${ms.id}">${img(preview({ layout: ms.layout, lineStyle: ms.lineStyle, shapes: ms.shapes, theme: s.theme, font: ms.font || s.font, pattern: 'none' }, `ms:${ms.id}:${s.theme}`), 54)}<span class="nm">${ms.name}</span></div>`;
      }).join('')}</div></div>
      <div class="sec"><h4>Colour theme</h4><div class="grid c2">${T.THEMES.map((th) => `<div class="card theme-card${s.theme === th.id ? ' on' : ''}" data-theme="${th.id}">${img(preview({ theme: th.id, layout: 'mindmap', lineStyle: s.lineStyle, shapes: s.shapes, pattern: 'none', rainbow: null }, `th:${th.id}:${s.lineStyle}:${JSON.stringify(s.shapes)}`), 56)}<span class="nm">${th.name}</span></div>`).join('')}</div></div>
      <div class="sec"><h4>Structure</h4><div class="grid c3">${T.LAYOUTS.map((l) => `<div class="card${s.layout === l.id ? ' on' : ''}" data-layout="${l.id}" title="${l.name}">${img(preview({ layout: l.id, theme: 'classic', lineStyle: l.id === 'down' || l.id === 'up' ? 'elbow' : 'curve', shapes: { root: 'rounded', main: 'rounded', sub: 'rounded' }, pattern: 'none' }, `lay:${l.id}`), 44)}<span class="nm">${l.name.replace(/ \(.*\)/, '')}</span></div>`).join('')}</div></div>
      <div class="sec"><h4>Branch lines</h4>${seg('lineStyle', s.lineStyle, T.LINE_STYLES.map((l) => [l.id, l.name.split(' ')[0]]))}
        ${range('lineWidth', 'Thickness', s.lineWidth, 1, 6, 0.5, '')}
        <div class="row"><label>Rainbow</label>${seg('rainbow', s.rainbow == null ? '' : s.rainbow ? 'on' : 'off', [['', 'Theme'], ['on', 'On'], ['off', 'Off']])}</div>
        <div class="row"><label>Numbering</label>${seg('numbering', s.numbering || 'off', [['off', 'Off'], ['on', '1 · 1.1 · 1.1.1']])}</div></div>
      <div class="sec"><h4>Topic shapes</h4>
        ${['root', 'main', 'sub'].map((lvl) => `<div class="row"><label>${{ root: 'Central', main: 'Main', sub: 'Subtopics' }[lvl]}</label><select class="field" data-shapelvl="${lvl}">${T.SHAPES.map((sh) => `<option value="${sh.id}"${s.shapes[lvl] === sh.id ? ' selected' : ''}>${sh.name}</option>`).join('')}</select></div>`).join('')}
      </div>
      <div class="sec"><h4>Background</h4><div class="grid c4">${T.PATTERNS.map((p) => {
        const pc = R.patternCSS(p.id, theme.ink, 1, 0, 0);
        return `<div class="card${s.pattern === p.id ? ' on' : ''}" data-pattern="${p.id}"><div style="width:100%;height:34px;border-radius:5px;background-color:${theme.bg};background-image:${pc.image};background-size:${pc.size};border:1px solid var(--ui-border)"></div><span class="nm">${p.name}</span></div>`;
      }).join('')}</div></div>
      <div class="sec"><h4>Typography & spacing</h4>
        <div class="row"><label>Font</label><select class="field" data-mapsel="font">${T.FONTS.map((x) => `<option value="${x.id}"${s.font === x.id ? ' selected' : ''}>${x.name}</option>`).join('')}</select></div>
        ${range('fontScale', 'Text size', s.fontScale, 0.7, 1.6, 0.05, '×')}
        ${range('hGap', 'Horizontal', s.hGap, 20, 160, 2, '')}
        ${range('vGap', 'Vertical', s.vGap, 2, 60, 1, '')}
        ${range('maxWidth', 'Topic width', s.maxWidth, 100, 600, 10, '')}
      </div>
      <div class="sec"><div class="toggle-row"><button class="btn" data-cmd="resetAllPositions">Reset hand-placed topics</button><button class="btn" data-clear-styles>Clear custom topic styles</button></div></div>`;
  }

  function markerPreview(m) {
    return `<svg viewBox="0 0 24 24">${R.markerSVG('marker', m, 2, 2, 20, '#333').replace(/data-act="marker"/g, '')}</svg>`;
  }
  function markersTab() {
    const n = App.primaryNode();
    const have = new Set((n && n.icons) || []);
    return (n ? '' : '<div class="sec"><p class="muted">Select one or more topics, then click a marker to add or remove it.</p></div>')
      + T.MARKER_GROUPS.map((g) => `<div class="sec"><h4>${g.name}</h4><div class="markers">${g.items.map((m) => `<button class="mk${have.has(m) ? ' on' : ''}" data-marker="${esc(m)}" title="${esc(m)}">${markerPreview(m)}</button>`).join('')}</div></div>`).join('')
      + `<div class="sec"><h4>Task checkbox</h4><button class="btn" data-cmd="task">Toggle task (☐ → ☑ → none)</button></div>
         <div class="sec"><button class="btn danger" data-clear-markers>Remove all markers from selection</button></div>`;
  }

  function notesTab() {
    const n = App.primaryNode();
    if (!n) return '<div class="sec"><p class="muted">Select a topic to write notes for it. Topics with notes show a 📝 icon; hover it to preview.</p></div>';
    return `<div class="sec"><h4>Notes for “${esc(n.text.slice(0, 40) || 'Topic')}”</h4><textarea class="field" id="note-area" placeholder="Write details, ideas, links…">${esc(n.note || '')}</textarea><p class="muted">Notes are exported to Markdown, OPML and FreeMind.</p></div>`;
  }

  function renderInspector(force) {
    const body = insp();
    if (!body || document.body.classList.contains('no-inspector')) return;
    if (!force && body.contains(document.activeElement) && App.isTextInput(document.activeElement)) return;
    const scroll = body.scrollTop;
    body.innerHTML = tab === 'map' ? mapTab() : tab === 'markers' ? markersTab() : tab === 'notes' ? notesTab() : topicTab();
    body.scrollTop = scroll;
  }
  App.renderInspector = renderInspector;

  function bindInspector() {
    const body = insp();
    document.getElementById('insp-tabs').addEventListener('click', (e) => { const b = e.target.closest('button[data-tab]'); if (b) showTab(b.dataset.tab); });
    body.addEventListener('click', (e) => {
      const el = e.target;
      const sw = el.closest('.sw:not(.custom)');
      if (sw) {
        const key = sw.parentElement.dataset.key;
        const val = sw.dataset.val;
        if (key === 'relColor') { setRel({ color: val }); return; }
        App.run('style', { [key]: val });
        return;
      }
      const card = el.closest('.card');
      if (card) {
        if (card.dataset.shape) App.run('style', { shape: card.dataset.shape });
        else if (card.dataset.theme) setSettings({ theme: card.dataset.theme, rainbow: null });
        else if (card.dataset.layout) setSettings({ layout: card.dataset.layout });
        else if (card.dataset.pattern) setSettings({ pattern: card.dataset.pattern });
        else if (card.dataset.mapstyle) {
          const ms = T.MAP_STYLES.find((x) => x.id === card.dataset.mapstyle);
          setSettings({ layout: ms.layout, lineStyle: ms.lineStyle, shapes: Object.assign({}, ms.shapes), font: ms.font || 'system' });
        }
        return;
      }
      const segBtn = el.closest('.seg button');
      if (segBtn) {
        const key = segBtn.parentElement.dataset.seg;
        const v = segBtn.dataset.val;
        const n = App.primaryNode();
        if (key === 'lineStyle') return setSettings({ lineStyle: v });
        if (key === 'rainbow') return setSettings({ rainbow: v === '' ? null : v === 'on' });
        if (key === 'numbering') return setSettings({ numbering: v });
        if (key === 'align') return App.run('style', { align: v });
        if (key === 'borderWidth') return App.run('style', { borderWidth: v === '' ? null : +v });
        if (key === 'borderDash') return App.run('style', { borderDash: v === 'solid' ? null : v });
        if (key === 'task') { App.selectedNodes().forEach((x) => { if (v) x.task = v; else delete x.task; }); return App.commit({ noScroll: true }); }
        if (key === 'boundary') { App.selectedNodes().forEach((x) => { if (v) x.boundary = true; else { delete x.boundary; delete x.boundaryLabel; } }); App.commit({ noScroll: true }); return renderInspector(true); }
        if (key === 'side' && n) { if (v) n.side = v; else delete n.side; return App.commit(); }
        return;
      }
      const tg = el.closest('[data-toggle]');
      if (tg) return App.run('toggleStyle', tg.dataset.toggle);
      const mk = el.closest('[data-marker]');
      if (mk) return App.run('marker', mk.dataset.marker);
      if (el.closest('[data-reset-style]')) { App.selectedNodes().forEach((n) => delete n.style); return App.commit({ noScroll: true }); }
      if (el.closest('[data-remove-image]')) { App.selectedNodes().forEach((n) => delete n.image); return App.commit({ noScroll: true }); }
      if (el.closest('[data-clear-due]')) { App.selectedNodes().forEach((n) => delete n.due); App.commit({ noScroll: true }); return renderInspector(true); }
      if (el.closest('[data-clear-markers]')) { App.selectedNodes().forEach((n) => delete n.icons); return App.commit({ noScroll: true }); }
      if (el.closest('[data-clear-styles]')) { M.walkDoc(App.doc, (n) => delete n.style); return App.commit({ noScroll: true }); }
    });
    body.addEventListener('input', (e) => {
      const el = e.target;
      if (el.dataset.range) {
        el.nextElementSibling.textContent = el.value + (el.nextElementSibling.textContent.replace(/[\d.]+/, '').trim() || '');
        applyRange(el.dataset.range, +el.value, false);
      } else if (el.id === 'note-area') {
        const n = App.primaryNode();
        if (n) { const had = !!n.note; n.note = el.value; if (!el.value) delete n.note; if (had !== !!n.note) App.preview(); }
      } else if (el.dataset.relLabel != null) {
        const r = App.doc.relations.find((x) => x.id === App.selRel);
        if (r) { r.label = el.value; App.preview(); }
      }
    });
    body.addEventListener('change', (e) => {
      const el = e.target;
      if (el.dataset.range) return applyRange(el.dataset.range, +el.value, true);
      if (el.type === 'color') {
        if (el.dataset.key === 'relColor') return setRel({ color: el.value });
        return App.run('style', { [el.dataset.key]: el.value });
      }
      if (el.dataset.sel === 'font') return App.run('style', { font: el.value || null });
      if (el.dataset.mapsel === 'font') return setSettings({ font: el.value });
      if (el.dataset.shapelvl) { const shapes = Object.assign({}, App.doc.settings.shapes, { [el.dataset.shapelvl]: el.value }); return setSettings({ shapes }); }
      if (el.dataset.field) {
        const k = el.dataset.field;
        App.selectedNodes().forEach((n) => {
          if (k === 'tags') { const tags = el.value.split(',').map((x) => x.trim()).filter(Boolean); if (tags.length) n.tags = tags; else delete n.tags; }
          else if (k === 'link') { let v = el.value.trim(); if (v && !/^[a-z]+:/i.test(v)) v = 'https://' + v; if (v) n.link = v; else delete n.link; }
          else if (el.value.trim()) n[k] = el.value.trim(); else delete n[k];
        });
        App.commit({ noScroll: true });
        return renderInspector(true);
      }
      if (el.id === 'note-area' || el.dataset.relLabel != null) App.commit({ noScroll: true });
    });
    body.addEventListener('focusout', (e) => { if (e.target.id === 'note-area') App.commit({ noScroll: true }); });
  }

  function applyRange(key, v, final) {
    const s = App.doc.settings;
    if (['lineWidth', 'fontScale', 'hGap', 'vGap', 'maxWidth'].includes(key)) {
      s[key] = v;
      if (final) { App.commit({ noScroll: true }); previewCache.clear(); } else App.preview();
      return;
    }
    const nodes = App.selectedNodes();
    nodes.forEach((n) => {
      if (key === 'imgW' && n.image) { const ratio = n.image.h / n.image.w; n.image.w = v; n.image.h = Math.round(v * ratio); return; }
      n.style = Object.assign({}, n.style || {}, { [key]: v });
    });
    if (final) App.commit({ noScroll: true }); else App.preview();
  }

  function setSettings(patch) {
    Object.assign(App.doc.settings, patch);
    App.commit({ noScroll: true });
    renderInspector(true);
    requestAnimationFrame(() => App.fit());
  }
  App.setSettings = setSettings;
  function setRel(patch) {
    const r = App.doc.relations.find((x) => x.id === App.selRel);
    if (!r) return;
    Object.assign(r, patch);
    App.commit({ noScroll: true });
    renderInspector(true);
  }

  // ================= outline =================
  let outlineQueued = false;
  function renderOutline() {
    if (outlineQueued) return;
    outlineQueued = true;
    requestAnimationFrame(() => {
      outlineQueued = false;
      if (document.body.classList.contains('no-outline')) return;
      const el = $('#outline-tree');
      const sel = new Set(App.sel);
      const rows = [];
      const theme = App.theme();
      const rec = (n, d, branch) => {
        const t = App.L.nodes.get(n.id);
        const color = t ? (t.box.style.fill !== 'transparent' && t.depth <= 1 ? t.box.style.fill : t.box.style.line) : theme.line;
        rows.push(`<div class="ol-row${sel.has(n.id) ? ' sel' : ''}" data-id="${n.id}" style="padding-left:${6 + d * 14}px"><span class="tw" data-tw>${n.children.length ? (n.collapsed ? '▶' : '▼') : ''}</span><span class="dot" style="background:${color}"></span><span class="tx${n.text ? '' : ' empty'}">${esc((n.text || 'Topic').replace(/\n/g, ' '))}</span></div>`);
        if (!n.collapsed) n.children.forEach((c) => rec(c, d + 1, branch));
      };
      M.trees(App.doc).forEach((t) => rec(t, 0, 0));
      el.innerHTML = rows.join('');
      const s = el.querySelector('.ol-row.sel');
      if (s) { const r = s.getBoundingClientRect(), pr = el.getBoundingClientRect(); if (r.top < pr.top || r.bottom > pr.bottom) s.scrollIntoView({ block: 'nearest' }); }
    });
  }
  function bindOutline() {
    const el = $('#outline-tree');
    el.addEventListener('click', (e) => {
      const row = e.target.closest('.ol-row');
      if (!row) return;
      const id = row.dataset.id;
      if (e.target.closest('[data-tw]')) { M.setCollapsed(App.doc, id); App.commit({ noScroll: true }); return; }
      if (App.focusId && !M.isAncestor(App.doc, App.focusId, id) && id !== App.focusId) App.run('focus');
      App.select(id, { noScroll: true });
      App.centerOn(id);
    });
    el.addEventListener('dblclick', (e) => { const row = e.target.closest('.ol-row'); if (row) App.startEdit(row.dataset.id); });
  }

  // ================= minimap =================
  function drawMinimap() {
    if (document.body.classList.contains('no-minimap') || !App.L) return;
    const cv = $('#minimap');
    const dpr = window.devicePixelRatio || 1;
    const W = 200, H = 130;
    if (cv.width !== W * dpr) { cv.width = W * dpr; cv.height = H * dpr; }
    const ctx = cv.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const theme = App.theme();
    ctx.fillStyle = theme.bg;
    ctx.fillRect(0, 0, W, H);
    const b = App.L.bounds, pad = 8;
    const k = Math.min((W - pad * 2) / Math.max(1, b.w), (H - pad * 2) / Math.max(1, b.h));
    const ox = pad + (W - pad * 2 - b.w * k) / 2 - b.minX * k, oy = pad + (H - pad * 2 - b.h * k) / 2 - b.minY * k;
    drawMinimap.map = { k, ox, oy };
    for (const t of App.L.list) {
      const st = t.box.style;
      ctx.fillStyle = st.fill && st.fill !== 'transparent' && st.fill !== theme.bg ? st.fill : st.line;
      ctx.fillRect(ox + t.x * k, oy + t.y * k, Math.max(2, t.w * k), Math.max(1.5, t.h * k));
    }
    const r = $('#stage').getBoundingClientRect(), v = App.view;
    const vx = (-v.x / v.zoom) * k + ox, vy = (-v.y / v.zoom) * k + oy, vw = (r.width / v.zoom) * k, vh = (r.height / v.zoom) * k;
    ctx.strokeStyle = '#3b82f6';
    ctx.lineWidth = 1.5;
    ctx.fillStyle = 'rgba(59,130,246,0.08)';
    ctx.fillRect(vx, vy, vw, vh);
    ctx.strokeRect(vx, vy, vw, vh);
  }
  function bindMinimap() {
    const cv = $('#minimap');
    let down = false;
    const go = (e) => {
      const m = drawMinimap.map;
      if (!m) return;
      const rr = cv.getBoundingClientRect();
      const wx = (e.clientX - rr.left - m.ox) / m.k, wy = (e.clientY - rr.top - m.oy) / m.k;
      const r = $('#stage').getBoundingClientRect();
      App.view.x = r.width / 2 - wx * App.view.zoom;
      App.view.y = r.height / 2 - wy * App.view.zoom;
      App.applyView();
    };
    cv.addEventListener('pointerdown', (e) => { e.stopPropagation(); down = true; cv.setPointerCapture(e.pointerId); go(e); });
    cv.addEventListener('pointermove', (e) => { if (down) go(e); });
    cv.addEventListener('pointerup', () => { down = false; });
    cv.addEventListener('dblclick', (e) => e.stopPropagation());
  }

  // ================= status bar + toolbar state =================
  function updateStatus() {
    const s = M.stats(App.doc);
    $('#st-stats').textContent = `${s.nodes} topics · ${s.words} words${s.tasks ? ` · ${s.done}/${s.tasks} tasks done (${Math.round((s.done / s.tasks) * 100)}%)` : ''}${s.overdue ? ` · ⚠ ${s.overdue} overdue` : ''}`;
    const q = (c) => document.querySelector(`#toolbar [data-cmd="${c}"]`);
    q('undo').disabled = !App.history.canUndo();
    q('redo').disabled = !App.history.canRedo();
    q('focus').classList.toggle('on', !!App.focusId);
    q('toggleOutline').classList.toggle('on', !document.body.classList.contains('no-outline'));
    q('toggleInspector').classList.toggle('on', !document.body.classList.contains('no-inspector'));
  }

  // ================= extra commands =================
  Object.assign(App.commands, {
    mapPanel() { showTab('map'); },
    markers() { showTab('markers'); },
    note() {
      if (!App.primary()) return App.toast('Select a topic first.');
      showTab('notes');
      setTimeout(() => { const a = $('#note-area'); if (a) { a.focus(); a.setSelectionRange(a.value.length, a.value.length); } }, 30);
    },
    async link() {
      const n = App.primaryNode();
      if (!n) return App.toast('Select a topic first.');
      const v = await App.prompt('Hyperlink', 'Web address for this topic (leave empty to remove):', n.link || '', 'https://example.com');
      if (v == null) return;
      let url = v.trim();
      if (url && !/^[a-z]+:/i.test(url)) url = 'https://' + url;
      App.selectedNodes().forEach((x) => { if (url) x.link = url; else delete x.link; });
      App.commit();
    },
    async image() {
      if (!App.primary()) return App.toast('Select a topic first.');
      const f = await P.pickFile('image/*');
      if (f) App.setImageFromFile(f);
    },
    async relationLabel() {
      const r = App.doc.relations.find((x) => x.id === App.selRel);
      if (!r) return;
      const v = await App.prompt('Relationship label', 'Describe how these topics relate:', r.label || '', 'e.g. depends on');
      if (v == null) return;
      r.label = v.trim();
      App.commit({ noScroll: true });
    },
    relationReverse() {
      const r = App.doc.relations.find((x) => x.id === App.selRel);
      if (!r) return;
      [r.from, r.to] = [r.to, r.from];
      App.commit({ noScroll: true });
    },
    exportMenu() {
      const b = document.querySelector('#toolbar [data-cmd="exportMenu"]').getBoundingClientRect();
      const run = (c, a) => () => App.run(c, a);
      App.showMenu([
        { label: 'PNG image', run: run('exportPNG') },
        { label: 'PNG (transparent)', run: () => App.run('exportPNG', true) },
        { label: 'SVG vector', run: run('exportSVG') },
        { label: 'PDF document', run: run('exportPDF') },
        '-',
        { label: 'Web page (.html)', run: run('exportHTML') },
        { label: 'Markdown (.md)', run: run('exportMarkdown') },
        { label: 'OPML outline', run: run('exportOPML') },
        { label: 'Plain text outline', run: run('exportText') },
        { label: 'FreeMind / Freeplane (.mm)', run: run('exportFreeMind') },
        { label: 'Spreadsheet (.csv)', run: run('exportCSV') },
        '-',
        { label: 'Copy map as image', run: run('copyImage') },
      ], b.left, b.bottom + 4);
    },
    async templates() {
      const recent = await P.recent();
      const cards = ML.templates.TEMPLATES.map((t) => {
        const d = ML.templates.build(t.id);
        const theme = T.getTheme(d.settings.theme);
        const L = Lay.layout(d, { theme, settings: d.settings, measure: App.measure });
        const out = R.exportSVG(L, d, { theme, settings: d.settings, padding: 20 });
        const url = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(out.svg);
        return `<button class="tpl" data-tpl="${t.id}"><div class="thumb" style="background:${theme.bg}"><img src="${url}" alt=""/></div><div class="meta"><b>${t.icon} ${t.name}</b><span>${t.desc}</span></div></button>`;
      }).join('');
      const rec = recent && recent.length ? `<div class="recent"><h3>Recent files</h3>${recent.slice(0, 8).map((p) => `<a data-recent="${esc(p)}" title="${esc(p)}">${esc(p.split(/[\\/]/).pop())} <span class="muted">— ${esc(p)}</span></a>`).join('')}</div>` : '';
      App.modal({
        title: 'Create a new mind map',
        body: `<div class="tpl-grid">${cards}</div>${rec}`,
        foot: '<span class="muted" style="margin-right:auto">Tip: you can also import Markdown, OPML, FreeMind and XMind files.</span><button class="btn" data-open>Open file…</button>',
        onMount(m, close) {
          m.addEventListener('click', async (e) => {
            const tb = e.target.closest('[data-tpl]');
            if (tb) {
              close();
              if (!(await App.confirmDiscard())) return;
              const d = ML.templates.build(tb.dataset.tpl);
              App.setDoc(d, { name: tb.dataset.tpl === 'blank' ? 'Untitled' : d.root.text });
              if (tb.dataset.tpl === 'blank') setTimeout(() => App.startEdit(App.doc.root.id), 350);
              return;
            }
            const r = e.target.closest('[data-recent]');
            if (r) { close(); App.run('openPath', r.dataset.recent); return; }
            if (e.target.closest('[data-open]')) { close(); App.run('open'); }
          });
        },
      });
    },
    shortcuts() {
      const K = App.K;
      const rows = [
        ['Add subtopic', 'Tab'], ['Add topic (sibling)', 'Enter'], ['Add topic before', 'Shift+Enter'], ['Add parent topic', K('Mod+Enter')],
        ['Edit topic', 'F2 / Space / just type'], ['New line while editing', 'Shift+Enter'], ['Delete topic', 'Delete / Backspace'],
        ['Navigate', 'Arrow keys'], ['Move topic up / down', 'Alt+↑ / Alt+↓'], ['Indent / outdent', 'Alt+→ / Alt+←'],
        ['Collapse / expand', K('Mod+/')], ['Duplicate', K('Mod+D')], ['Copy / cut / paste', K('Mod+C / X / V')],
        ['Undo / redo', K('Mod+Z / Mod+Shift+Z')], ['Bold / italic / underline', K('Mod+B / I / U')], ['Priority 1–6', K('Mod+1 … 6')],
        ['Task checkbox', K('Mod+Shift+T')], ['Hyperlink', K('Mod+K')], ['Note', K('Mod+Shift+M')], ['Relationship', K('Mod+Shift+R')],
        ['Due date', 'Right-click → Set due date'], ['Find & replace', K('Mod+F') + ' → Replace all'],
        ['Boundary', K('Mod+Shift+B')], ['Focus on branch', K('Mod+Shift+F')], ['Find', K('Mod+F')], ['Zoom in / out', K('Mod+= / Mod+-')],
        ['Fit map', K('Mod+0')], ['Toggle outline / format panel', K('Mod+\\ / Mod+Shift+\\')], ['Presentation', 'F5'], ['New / Open / Save', K('Mod+N / O / S')],
        ['Nudge a topic', 'Shift+Alt+arrows'], ['Copy / paste style', K('Mod+Alt+C / Mod+Alt+V')],
        ['Floating topic', 'Double-click canvas'], ['Select with a rectangle', 'Shift+drag canvas'], ['Pan', 'Drag canvas / scroll'], ['Zoom', K('Mod+scroll / pinch')],
      ];
      App.modal({
        title: 'Keyboard shortcuts',
        body: `<div class="cols"><table class="kbd-table">${rows.map(([a, b]) => `<tr><td>${a}</td><td>${b.split(' / ').map((x) => `<kbd>${esc(x)}</kbd>`).join(' / ')}</td></tr>`).join('')}</table></div>`,
      });
    },
    about() {
      App.modal({
        title: 'About Mindloom', cls: 'small',
        body: `<p><b>Mindloom</b> — a fast, friendly mind mapping app for macOS and Windows.</p><p class="muted">Version ${esc((window.mindloom && window.mindloom.version) || 'web')}. Open source under the MIT licence.</p>`,
      });
    },
  });

  // ================= AI assistant (bring your own key) =================
  App.commands.ai = async function (preset) {
    if (!P.isElectron) {
      App.modal({ title: '✨ AI assistant', cls: 'small', body: '<p>The AI assistant is part of the desktop app for macOS and Windows, where your API keys can be stored securely.</p>' });
      return;
    }
    let cfg = await P.aiConfig();
    const node = App.primaryNode();
    const st = { mode: preset === 'expand' && node ? 'expand' : 'generate', depth: 'standard', models: [] };
    const prov = () => cfg.providers.find((x) => x.id === cfg.provider) || cfg.providers[0];
    const groups = ['Cloud', 'Local', 'Custom'];
    const providerOptions = groups.map((g) => `<optgroup label="${g === 'Cloud' ? 'Cloud providers' : g === 'Local' ? 'On your computer' : 'Other'}">${cfg.providers.filter((x) => x.group === g).map((x) => `<option value="${x.id}">${esc(x.name)}</option>`).join('')}</optgroup>`).join('');
    const body = `
      <div class="ai-grid">
        <label>Provider</label><select class="field" id="ai-prov">${providerOptions}</select>
        <label id="ai-key-label">API key</label><div id="ai-key-row" class="row" style="margin:0"></div>
        <label id="ai-base-label">Address</label><div id="ai-base-row" class="row" style="margin:0"><input class="field" id="ai-base" spellcheck="false" placeholder="https://api.example.com/v1"/></div>
        <label>Model</label><div class="row" style="margin:0"><input class="field" id="ai-model" list="ai-model-list" spellcheck="false" placeholder="Model name"/><datalist id="ai-model-list"></datalist><button class="btn" id="ai-load" title="Fetch the models your key can use">Load models</button></div>
      </div>
      <div class="seg" style="margin:16px 0 10px" data-ai-mode>
        <button data-val="generate">From a topic</button>
        <button data-val="text">From my notes</button>
        <button data-val="expand"${node ? '' : ' disabled title="Select a topic first"'}>Expand selected topic</button>
      </div>
      <div data-pane="generate"><input class="field" id="ai-topic" placeholder="What is the map about? e.g. Launching a podcast"/></div>
      <div data-pane="text" hidden><textarea class="field" id="ai-text" style="min-height:140px" placeholder="Paste notes, an article, a transcript or meeting minutes. The AI turns them into a mind map."></textarea></div>
      <div data-pane="expand" hidden><p class="muted" style="margin:4px 0">Suggest new subtopics for <b>${esc(node ? node.text || 'Topic' : '')}</b>, using the rest of the map as context.</p></div>
      <div class="row" id="ai-opts"><label>Detail</label>${'<div class="seg" data-ai-depth><button data-val="quick">Quick</button><button data-val="standard" class="on">Standard</button><button data-val="deep">Deep</button></div>'}</div>
      <label class="row" id="ai-into-row" style="cursor:pointer"><input type="checkbox" id="ai-into"/> <span>Add to this map instead of starting a new one</span></label>
      <div id="ai-status" class="muted" style="margin-top:8px;min-height:18px"></div>
      <p class="muted" id="ai-privacy" style="margin:6px 0 0;font-size:11.5px"></p>`;
    App.modal({
      title: '✨ AI assistant', body,
      foot: '<button class="btn" data-x>Cancel</button><button class="btn primary" data-go>Generate</button>',
      onMount(m, close) {
        const $m = (q) => m.querySelector(q);
        const status = (html, isErr) => { $m('#ai-status').innerHTML = isErr ? `<span style="color:var(--ui-danger)">⚠️ ${esc(html)}</span>` : html; };
        const refreshCfg = async () => { cfg = await P.aiConfig(); };

        function renderProvider() {
          const p = prov();
          $m('#ai-prov').value = p.id;
          const keyRow = $m('#ai-key-row');
          if (p.noKey) {
            keyRow.innerHTML = `<span class="muted">No key needed — it runs on your computer. <a href="#" data-open="${esc(p.keyUrl)}">Get ${esc(p.name.split(' ')[0])}</a></span>`;
          } else if (p.hasKey) {
            keyRow.innerHTML = `<span style="color:#2f9e44;font-weight:600">✓ Key saved</span><button class="btn" data-key-replace>Replace</button><button class="btn danger" data-key-remove>Remove</button>`;
          } else {
            keyRow.innerHTML = `<input class="field" id="ai-key" type="password" autocomplete="off" spellcheck="false" placeholder="${esc(p.keyHint || 'Paste your API key')}${p.keyOptional ? ' (optional)' : ''}"/><button class="btn" data-key-save>Save</button>${p.keyUrl ? `<a href="#" data-open="${esc(p.keyUrl)}" style="white-space:nowrap">Get a key</a>` : ''}`;
          }
          $m('#ai-base-label').hidden = $m('#ai-base-row').hidden = !p.editableBase;
          $m('#ai-base').value = cfg.baseUrls[p.id] || p.base || '';
          $m('#ai-model').value = cfg.models[p.id] || p.defaultModel || '';
          $m('#ai-model-list').innerHTML = '';
          st.models = [];
          $m('#ai-privacy').textContent = p.group === 'Local'
            ? 'Your map stays on this computer: the request goes to the local model server only.'
            : `Your request is sent straight from this computer to ${p.name}. Keys are stored encrypted on this computer and are never shared with anyone else.`;
          status('');
          if (p.noKey || p.hasKey || (p.keyOptional && (cfg.baseUrls[p.id] || p.base))) loadModels(true);
        }

        async function loadModels(quiet) {
          const p = prov();
          if (!quiet) status('<span class="spinner"></span> Loading models…');
          try {
            const list = await P.aiModels(p.id);
            st.models = list;
            $m('#ai-model-list').innerHTML = list.map((x) => `<option value="${esc(x.id)}">${esc(x.name && x.name !== x.id ? x.name : '')}</option>`).join('');
            const cur = $m('#ai-model').value.trim();
            if (!cur && list.length) { $m('#ai-model').value = list[0].id; await P.aiSetConfig({ for: p.id, model: list[0].id }); }
            status(list.length ? `${list.length} models available${cur && !list.some((x) => x.id === cur) ? ' — the current model is not in the list, but you can still use it' : ''}.` : 'The provider returned no chat models. You can type a model name.');
          } catch (err) { if (!quiet) status(err.message, true); else status(''); }
        }

        $m('#ai-prov').addEventListener('change', async (e) => { await P.aiSetConfig({ provider: e.target.value }); await refreshCfg(); renderProvider(); });
        $m('#ai-base').addEventListener('change', async (e) => {
          try { await P.aiSetConfig({ for: prov().id, baseUrl: e.target.value }); await refreshCfg(); status('Address saved.'); loadModels(true); } catch (err) { status(err.message, true); }
        });
        $m('#ai-model').addEventListener('change', (e) => P.aiSetConfig({ for: prov().id, model: e.target.value }));
        $m('#ai-load').onclick = () => loadModels(false);
        const saveKey = async () => {
          const inp = $m('#ai-key');
          if (!inp || !inp.value.trim()) return true;
          try { await P.aiSetKey(prov().id, inp.value.trim()); await refreshCfg(); renderProvider(); status('Key saved securely.'); return true; } catch (err) { status(err.message, true); return false; }
        };
        m.addEventListener('click', async (e) => {
          const o = e.target.closest('[data-open]');
          if (o) { e.preventDefault(); P.openExternal(o.dataset.open); return; }
          if (e.target.closest('[data-key-save]')) { await saveKey(); return; }
          if (e.target.closest('[data-key-replace]')) { const p = prov(); p.hasKey = false; renderProvider(); setTimeout(() => $m('#ai-key') && $m('#ai-key').focus(), 0); return; }
          if (e.target.closest('[data-key-remove]')) { await P.aiClearKey(prov().id); await refreshCfg(); renderProvider(); status('Key removed from this computer.'); }
        });
        m.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.id === 'ai-key') { e.preventDefault(); saveKey(); } });

        const setMode = (mode) => {
          st.mode = mode;
          m.querySelectorAll('[data-ai-mode] button').forEach((x) => x.classList.toggle('on', x.dataset.val === mode));
          m.querySelectorAll('[data-pane]').forEach((x) => { x.hidden = x.dataset.pane !== mode; });
          $m('#ai-opts').hidden = mode === 'expand';
          $m('#ai-into-row').hidden = mode === 'expand';
          $m('[data-go]').textContent = mode === 'expand' ? 'Suggest subtopics' : 'Generate map';
          const f = mode === 'generate' ? $m('#ai-topic') : mode === 'text' ? $m('#ai-text') : null;
          if (f) setTimeout(() => f.focus(), 0);
        };
        m.querySelector('[data-ai-mode]').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b && !b.disabled) setMode(b.dataset.val); });
        m.querySelector('[data-ai-depth]').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; st.depth = b.dataset.val; m.querySelectorAll('[data-ai-depth] button').forEach((x) => x.classList.toggle('on', x === b)); });
        $m('[data-x]').onclick = () => close();

        async function go() {
          const p = prov();
          if (!(await saveKey())) return;
          if (!p.noKey && !p.keyOptional && !prov().hasKey) { status(`Paste your ${p.name} API key first.`, true); return; }
          const model = $m('#ai-model').value.trim();
          if (!model) { status('Choose a model first (press “Load models”).', true); return; }
          const req = { provider: p.id, model, mode: st.mode, depth: st.depth };
          if (st.mode === 'generate') { req.topic = $m('#ai-topic').value.trim(); if (!req.topic) { $m('#ai-topic').focus(); return; } }
          if (st.mode === 'text') { req.text = $m('#ai-text').value.trim(); if (req.text.length < 20) { status('Paste a bit more text (at least a sentence or two).', true); return; } }
          if (st.mode === 'expand') Object.assign(req, { topic: node.text, path: M.pathTo(App.doc, node.id).map((x) => x.text), existing: node.children.map((c) => c.text), outline: IO.toText(App.doc).slice(0, 6000) });
          const btn = $m('[data-go]');
          btn.disabled = true;
          status(`<span class="spinner"></span> Asking ${esc(p.name)} (${esc(model)})…`);
          try {
            const res = await P.ai(req);
            const into = $m('#ai-into').checked;
            close();
            if (st.mode === 'expand') {
              const added = IO.parseIndented(res.outline).map((r) => M.insertSubtree(App.doc, node.id, r)).filter(Boolean);
              App.sel = [node.id];
              App.commit();
              App.toast(`Added ${added.length} subtopic${added.length === 1 ? '' : 's'}`);
              return;
            }
            const d = IO.fromText(res.outline);
            if (into) {
              const target = App.primary() || App.doc.root.id;
              const n = M.insertSubtree(App.doc, target, d.root);
              App.sel = [n.id];
              App.commit();
              App.toast('Added the generated branch to your map');
              return;
            }
            if (!(await App.confirmDiscard())) return;
            const keep = ['theme', 'layout', 'lineStyle', 'shapes', 'pattern', 'font', 'rainbow'];
            for (const k of keep) d.settings[k] = M.clone(App.doc.settings[k]);
            App.setDoc(d, { dirty: true });
            App.toast('Map generated — edit anything you like');
          } catch (err) {
            btn.disabled = false;
            status(err.message || 'The request failed.', true);
          }
        }
        $m('[data-go]').onclick = go;
        $m('#ai-topic').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); go(); } });
        renderProvider();
        setMode(st.mode);
      },
    });
  };

  // ================= wiring =================
  App.initPanels = function () {
    bindInspector();
    bindOutline();
    bindMinimap();
    App.on('render', () => { renderInspector(); renderOutline(); drawMinimap(); updateStatus(); });
    App.on('select', () => { renderInspector(true); renderOutline(); });
    App.on('doc', () => { previewCache.clear(); renderInspector(true); });
    App.on('view', drawMinimap);
    const origToggleOutline = App.commands.toggleOutline;
    App.commands.toggleOutline = () => { origToggleOutline(); renderOutline(); updateStatus(); };
    const origToggleInspector = App.commands.toggleInspector;
    App.commands.toggleInspector = () => { origToggleInspector(); renderInspector(true); updateStatus(); };
  };
})(window.ML = window.ML || {});
