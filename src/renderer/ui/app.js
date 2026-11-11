/* Mindloom — application controller: state, rendering, input, files */
(function (ML) {
  'use strict';
  const M = ML.model, Lay = ML.layout, R = ML.render, T = ML.themes, IO = ML.io, P = ML.platform;
  const $ = (s) => document.querySelector(s);

  const App = {
    doc: null, path: null, name: 'Untitled', dirty: false,
    sel: [], selRel: null,
    view: { zoom: 1, x: 0, y: 0 },
    focusId: null, L: null,
    history: new M.History(),
    editing: null, mode: null, clipboard: null,
    matches: [], matchIdx: -1, present: null,
    commands: {}, listeners: {},
  };
  ML.app = App;

  let stage, svg, vp, dragLayer, editor;

  // ---------- events ----------
  App.on = (ev, fn) => { (App.listeners[ev] = App.listeners[ev] || []).push(fn); };
  App.emit = (ev, a) => { (App.listeners[ev] || []).forEach((fn) => { try { fn(a); } catch (e) { console.error(e); } }); };

  // ---------- text measuring ----------
  const mctx = document.createElement('canvas').getContext('2d');
  const mcache = new Map();
  function measure(text, font) {
    const k = font + '\u0001' + text;
    let v = mcache.get(k);
    if (v == null) {
      mctx.font = font;
      v = mctx.measureText(text).width;
      if (mcache.size > 30000) mcache.clear();
      mcache.set(k, v);
    }
    return v;
  }
  App.measure = measure;

  // ---------- helpers ----------
  App.theme = () => T.getTheme(App.doc.settings.theme);
  App.primary = () => App.sel[App.sel.length - 1] || null;
  App.primaryNode = () => (App.primary() ? M.getNode(App.doc, App.primary()) : null);
  App.selectedNodes = () => App.sel.map((id) => M.getNode(App.doc, id)).filter(Boolean);
  App.isRoot = (id) => id === App.doc.root.id;

  function toast(msg, ms) {
    const el = $('#toast');
    el.textContent = msg;
    el.hidden = false;
    clearTimeout(toast.t);
    toast.t = setTimeout(() => { el.hidden = true; }, ms || 2200);
  }
  App.toast = toast;

  // ---------- document lifecycle ----------
  App.setDoc = function (doc, opts) {
    opts = opts || {};
    App.doc = M.normalizeDoc(doc);
    App.path = opts.path || null;
    App.name = opts.name || App.doc.root.text.split('\n')[0].slice(0, 60) || 'Untitled';
    App.focusId = null;
    App.selRel = null;
    App.matches = [];
    placeImportedFloating();
    App.history.reset(snapshot());
    App.sel = [App.doc.root.id];
    setDirty(!!opts.dirty);
    render();
    requestAnimationFrame(() => { App.fit(false); });
    App.emit('doc');
  };

  /** Imported floating topics have no coordinates yet: line them up under the main map. */
  function placeImportedFloating() {
    const todo = App.doc.floating.filter((f) => f._place);
    if (!todo.length) return;
    const main = Lay.layout(Object.assign({}, App.doc, { floating: [] }), { theme: App.theme(), settings: App.doc.settings, measure });
    let x = main.bounds.minX;
    const y = main.bounds.maxY + 70;
    for (const f of todo) {
      const one = Lay.layout(Object.assign({}, App.doc, { root: f, floating: [] }), { theme: App.theme(), settings: Object.assign({}, App.doc.settings, { layout: 'right' }), measure });
      f.x = Math.round(x); f.y = Math.round(y + (one.bounds.h - one.root.h) / 2);
      x += one.bounds.w + 60;
      delete f._place;
    }
  }

  function snapshot() { return JSON.stringify(App.doc); }

  /** Call after any document mutation: records undo, marks dirty, renders, schedules autosave. */
  App.commit = function (opts) {
    if (App.history.push(snapshot())) setDirty(true);
    render();
    if (!(opts && opts.noScroll) && App.primary()) App.ensureVisible(App.primary());
    scheduleAutosave();
  };
  /** Live preview (no history) — used by sliders while dragging. */
  App.preview = function () { render(); };

  function restore(snap) {
    const sel = App.sel.slice();
    App.doc = JSON.parse(snap);
    App.sel = sel.filter((id) => M.getNode(App.doc, id));
    if (!App.sel.length) App.sel = [App.doc.root.id];
    if (App.focusId && !M.getNode(App.doc, App.focusId)) App.focusId = null;
    App.selRel = null;
    setDirty(true);
    render();
    scheduleAutosave();
    App.emit('doc');
  }

  function setDirty(v) {
    App.dirty = v;
    document.body.classList.toggle('dirty', v);
    $('#doc-name').textContent = App.name;
    P.setTitle(`${App.name}${v ? ' •' : ''} — Mindloom`, v, App.path);
  }
  App.setDirty = setDirty;

  let autosaveT;
  function scheduleAutosave() {
    clearTimeout(autosaveT);
    autosaveT = setTimeout(async () => {
      const ok = await P.autosave({ doc: App.doc, path: App.path, name: App.name, dirty: App.dirty, t: Date.now() });
      $('#st-save').textContent = ok ? 'Auto-saved ' + new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : 'Auto-save unavailable (map too large) — save the file to keep your work';
    }, 700);
  }

  // ---------- rendering ----------
  function render() {
    const theme = App.theme();
    const s = App.doc.settings;
    App.L = Lay.layout(App.doc, { theme, settings: s, measure, focusId: App.focusId });
    // remember which side each main topic is on, so later edits never reshuffle the map
    if (s.layout === 'mindmap' && !App.focusId) for (const c of App.L.root.children) if (!c.node.side && c.side) c.node.side = c.side;
    vp.innerHTML = R.scene(App.L, App.doc, {
      theme, settings: s, selected: new Set(App.sel), matches: new Set(App.matches), selectedRel: App.selRel,
    });
    for (const id of App.sel) { const g = vp.querySelector(`.node[data-id="${id}"]`); if (g) g.classList.add('sel'); }
    stage.style.backgroundColor = theme.bg;
    applyView();
    if (!App.dragging) App.emit('render');
  }
  App.render = render;

  function applyView() {
    const v = App.view;
    const tf = `translate(${v.x} ${v.y}) scale(${v.zoom})`;
    vp.setAttribute('transform', tf);
    dragLayer.setAttribute('transform', tf);
    const pc = R.patternCSS(App.doc.settings.pattern, App.theme().ink, v.zoom, v.x, v.y);
    stage.style.backgroundImage = pc.image;
    stage.style.backgroundSize = pc.size;
    stage.style.backgroundPosition = pc.position;
    $('#zoom-label').textContent = Math.round(v.zoom * 100) + '%';
    if (App.editing) positionEditor();
    App.emit('view');
  }
  App.applyView = applyView;

  const stageRect = () => stage.getBoundingClientRect();
  function toWorld(clientX, clientY) {
    const r = stageRect(), v = App.view;
    return { x: (clientX - r.left - v.x) / v.zoom, y: (clientY - r.top - v.y) / v.zoom };
  }
  App.toWorld = toWorld;

  let anim = null, animTarget = null;
  function animateView(target, ms) {
    cancelAnimationFrame(anim);
    const from = Object.assign({}, App.view);
    const t0 = performance.now();
    const dur = ms == null ? 260 : ms;
    animTarget = null;
    if (!dur) { Object.assign(App.view, target); applyView(); return; }
    animTarget = Object.assign({}, target);
    const step = (now) => {
      const k = Math.min(1, (now - t0) / dur), e = 1 - Math.pow(1 - k, 3);
      App.view.x = from.x + (target.x - from.x) * e;
      App.view.y = from.y + (target.y - from.y) * e;
      App.view.zoom = from.zoom + (target.zoom - from.zoom) * e;
      applyView();
      if (k < 1) anim = requestAnimationFrame(step); else animTarget = null;
    };
    anim = requestAnimationFrame(step);
  }
  /** Finish any running view animation now, so the map never slides under the pointer. */
  App.settleView = function () {
    if (!animTarget) return;
    cancelAnimationFrame(anim);
    Object.assign(App.view, animTarget);
    animTarget = null;
    applyView();
  };

  App.fitBox = function (b, animate, maxZoom, pad) {
    const r = stageRect();
    pad = pad == null ? 60 : pad;
    const zoom = Math.max(0.08, Math.min(maxZoom || 1.2, (r.width - pad * 2) / Math.max(1, b.w), (r.height - pad * 2) / Math.max(1, b.h)));
    const cx = b.minX + b.w / 2, cy = b.minY + b.h / 2;
    animateView({ zoom, x: r.width / 2 - cx * zoom, y: r.height / 2 - cy * zoom }, animate === false ? 0 : 300);
  };
  App.fit = (animate) => App.fitBox(App.L.bounds, animate, 1.1);

  App.zoomAt = function (factor, cx, cy) {
    const r = stageRect(), v = App.view;
    if (cx == null) { cx = r.width / 2; cy = r.height / 2; }
    const z = Math.max(0.08, Math.min(4, v.zoom * factor));
    const wx = (cx - v.x) / v.zoom, wy = (cy - v.y) / v.zoom;
    v.zoom = z; v.x = cx - wx * z; v.y = cy - wy * z;
    applyView();
  };

  App.centerOn = function (id, animate) {
    const t = App.L.nodes.get(id);
    if (!t) return;
    const r = stageRect(), v = App.view;
    animateView({ zoom: v.zoom, x: r.width / 2 - (t.x + t.w / 2) * v.zoom, y: r.height / 2 - (t.y + t.h / 2) * v.zoom }, animate === false ? 0 : 250);
  };

  App.ensureVisible = function (id) {
    const t = App.L && App.L.nodes.get(id);
    if (!t) return;
    const r = stageRect(), v = App.view, m = 50;
    const x1 = t.x * v.zoom + v.x, y1 = t.y * v.zoom + v.y, x2 = x1 + t.w * v.zoom, y2 = y1 + t.h * v.zoom;
    let dx = 0, dy = 0;
    if (x1 < m) dx = m - x1; else if (x2 > r.width - m) dx = Math.max(m - x1, r.width - m - x2);
    if (y1 < m) dy = m - y1; else if (y2 > r.height - m) dy = Math.max(m - y1, r.height - m - y2);
    if (dx || dy) animateView({ zoom: v.zoom, x: v.x + dx, y: v.y + dy }, 180);
  };

  // ---------- selection ----------
  App.select = function (ids, opts) {
    ids = Array.isArray(ids) ? ids : ids ? [ids] : [];
    if (opts && opts.toggle) {
      const set = new Set(App.sel);
      for (const id of ids) { if (set.has(id)) set.delete(id); else set.add(id); }
      App.sel = [...set];
    } else App.sel = ids;
    App.selRel = null;
    render();
    if (App.primary() && !(opts && opts.noScroll)) App.ensureVisible(App.primary());
    App.emit('select');
  };

  // ---------- inline editing ----------
  App.startEdit = function (id, opts) {
    opts = opts || {};
    const node = M.getNode(App.doc, id);
    if (!node || !App.L.nodes.get(id)) return;
    if (App.editing) App.commitEdit();
    App.sel = [id];
    App.editing = { id, original: node.text };
    editor.value = opts.initial != null ? opts.initial : node.text;
    if (opts.initial != null) { node.text = opts.initial; render(); }
    editor.style.display = 'block';
    positionEditor();
    editor.focus();
    if (opts.initial == null) editor.select();
    else editor.setSelectionRange(editor.value.length, editor.value.length);
    App.ensureVisible(id);
    App.emit('select');
  };

  function positionEditor() {
    const ed = App.editing;
    const t = ed && App.L.nodes.get(ed.id);
    if (!t) return;
    const v = App.view, b = t.box, st = b.style;
    const x = (t.x + b.row.x + b.row.textX) * v.zoom + v.x;
    const y = (t.y + b.row.y + (b.row.h - b.row.textH) / 2) * v.zoom + v.y;
    const fs = st.fontSize * v.zoom;
    editor.style.font = `${st.italic ? 'italic ' : ''}${st.bold ? 700 : 400} ${fs}px ${st.font}`;
    editor.style.lineHeight = `${b.lh * v.zoom}px`;
    editor.style.textAlign = st.align;
    const w = Math.max(b.row.textW * v.zoom, 40) + 14;
    editor.style.width = w + 'px';
    editor.style.left = x - 7 + 'px';
    editor.style.top = y - 4 + 'px';
    editor.style.height = 'auto';
    editor.style.height = Math.max(b.row.textH * v.zoom + 8, editor.scrollHeight) + 'px';
  }

  App.commitEdit = function () {
    const ed = App.editing;
    if (!ed) return;
    App.editing = null;
    editor.style.display = 'none';
    const node = M.getNode(App.doc, ed.id);
    if (node) node.text = editor.value.replace(/\s+$/, '');
    stage.focus({ preventScroll: true });
    if (node && node.text !== ed.original) {
      if (!App.path && ed.id === App.doc.root.id) App.name = App.doc.root.text.split('\n')[0].slice(0, 60) || 'Untitled';
      App.commit();
    }
    else render();
  };

  App.cancelEdit = function () {
    const ed = App.editing;
    if (!ed) return;
    App.editing = null;
    editor.style.display = 'none';
    const node = M.getNode(App.doc, ed.id);
    if (node) node.text = ed.original;
    stage.focus({ preventScroll: true });
    render();
  };

  // ---------- structural commands ----------
  function defaultText(parentId) {
    const f = M.find(App.doc, parentId);
    if (!f) return 'Topic';
    const depth = f.depth + 1 + (f.floating ? 1 : 0);
    return depth <= 1 ? 'Main Topic' : 'Subtopic';
  }
  function afterCreate(n) {
    App.sel = [n.id];
    App.commit();
    App.startEdit(n.id);
  }
  Object.assign(App.commands, {
    addChild() {
      const id = App.primary() || App.doc.root.id;
      afterCreate(M.addChild(App.doc, id, defaultText(id)));
    },
    addSibling(before) {
      const id = App.primary() || App.doc.root.id;
      const f = M.find(App.doc, id);
      if (!f) return;
      if (App.focusId === id) return App.commands.addChild();
      const parentId = f.parent ? f.parent.id : id;
      const n = M.addSibling(App.doc, id, f.parent ? defaultText(parentId) : f.floating ? 'Floating Topic' : 'Main Topic', before === true);
      if (f.floating && !f.parent) {
        // a new floating topic goes just below the whole branch of the one it was created from
        const t = App.L.nodes.get(id);
        if (t) { const b = Lay.subtreeBox(t); n.x = Math.round(t.x); n.y = Math.round(b.maxY + 30); }
      }
      afterCreate(n);
    },
    addParent() {
      const id = App.primary();
      if (!id || App.isRoot(id)) return;
      const n = M.addParent(App.doc, id, 'Topic');
      if (n) afterCreate(n);
    },
    deleteSel() {
      if (App.selRel) {
        App.doc.relations = App.doc.relations.filter((r) => r.id !== App.selRel);
        App.selRel = null;
        App.commit();
        return;
      }
      const ids = App.sel.filter((id) => !App.isRoot(id) && id !== App.focusId);
      if (!ids.length) return;
      const next = M.remove(App.doc, ids);
      App.sel = next ? [next] : [App.doc.root.id];
      App.commit();
    },
    edit() { if (App.primary()) App.startEdit(App.primary()); },
    duplicate() {
      const id = App.primary();
      if (!id) return;
      const c = M.duplicate(App.doc, id);
      if (c) { App.sel = [c.id]; App.commit(); }
    },
    moveUp() { if (App.primary() && M.reorder(App.doc, App.primary(), -1)) App.commit(); },
    moveDown() { if (App.primary() && M.reorder(App.doc, App.primary(), 1)) App.commit(); },
    indent() { if (App.primary() && M.indent(App.doc, App.primary())) App.commit(); },
    outdent() { if (App.primary() && M.outdent(App.doc, App.primary())) App.commit(); },
    toggleCollapse() {
      let changed = false;
      for (const id of App.sel) changed = M.setCollapsed(App.doc, id) || changed;
      if (changed) App.commit();
    },
    collapseAll() { App.doc.root.children.forEach((c) => M.walk(c, (n) => { if (n.children.length) n.collapsed = true; })); App.sel = [App.doc.root.id]; App.commit(); App.fit(); },
    expandAll() { M.walkDoc(App.doc, (n) => { n.collapsed = false; }); App.commit(); App.fit(); },
    collapseLevel(level) { M.collapseToLevel(App.doc, level); App.commit(); App.fit(); },
    sortChildren() {
      const n = App.primaryNode();
      if (!n || n.children.length < 2) return;
      n.children.sort((a, b) => a.text.localeCompare(b.text, undefined, { numeric: true, sensitivity: 'base' }));
      App.commit();
    },
    resetPosition() {
      let n = 0;
      for (const node of App.selectedNodes()) if (node.offset) { delete node.offset; n++; }
      if (n) App.commit({ noScroll: true }); else toast('That topic is already in its automatic position.');
    },
    resetAllPositions() {
      let n = 0;
      M.walkDoc(App.doc, (node) => { if (node.offset) { delete node.offset; n++; } });
      if (n) { App.commit({ noScroll: true }); toast(`Reset ${n} hand-placed topic${n > 1 ? 's' : ''}`); App.fit(); }
      else toast('No topics have been moved by hand.');
    },
    nudge(dir) {
      const key = App.doc.settings.layout;
      const step = 10;
      const dx = dir === 'left' ? -step : dir === 'right' ? step : 0, dy = dir === 'up' ? -step : dir === 'down' ? step : 0;
      let moved = false;
      for (const id of topLevelOf(App.sel)) {
        const f = M.find(App.doc, id);
        if (!f || (!f.parent && !f.floating) || id === App.focusId) continue;
        if (f.floating && !f.parent) { f.node.x = (f.node.x || 0) + dx; f.node.y = (f.node.y || 0) + dy; moved = true; continue; }
        const k = f.floating ? 'float' : key;
        if (!f.floating && !Lay.canFreePosition(key, f.depth)) continue;
        const o = f.node.offset && f.node.offset.l === k ? f.node.offset : { x: 0, y: 0 };
        const nx = o.x + dx, ny = o.y + dy;
        if (!nx && !ny) delete f.node.offset; else f.node.offset = { x: nx, y: ny, l: k };
        moved = true;
      }
      if (moved) App.commit();
    },
    detach() {
      const id = App.primary();
      const t = App.L.nodes.get(id);
      if (!t || !M.find(App.doc, id).parent) return;
      M.detach(App.doc, id, Math.round(t.x + 40), Math.round(t.y + 40));
      App.commit();
    },
    floatingAt(pt) {
      const n = M.createNode('Floating Topic', { x: Math.round(pt.x - 60), y: Math.round(pt.y - 18) });
      App.doc.floating.push(n);
      afterCreate(n);
    },
    undo() {
      if (isTextInput(document.activeElement)) { document.execCommand('undo'); return; }
      const s = App.history.undo();
      if (s) restore(s);
    },
    redo() {
      if (isTextInput(document.activeElement)) { document.execCommand('redo'); return; }
      const s = App.history.redo();
      if (s) restore(s);
    },
    boundary() {
      const nodes = App.selectedNodes();
      if (!nodes.length) return;
      const on = !nodes[0].boundary;
      nodes.forEach((n) => { if (on) n.boundary = true; else { delete n.boundary; delete n.boundaryLabel; } });
      App.commit();
    },
    relation() {
      const id = App.primary();
      if (!id) return toast('Select a topic first, then choose Relationship.');
      App.mode = { type: 'relation', from: id };
      stage.classList.add('relation-mode');
      const h = $('#mode-hint');
      h.textContent = 'Click the topic to connect to · Esc to cancel';
      h.hidden = false;
    },
    task() {
      const nodes = App.selectedNodes();
      if (!nodes.length) return;
      const next = !nodes[0].task ? 'todo' : nodes[0].task === 'todo' ? 'done' : null;
      nodes.forEach((n) => { if (next) n.task = next; else delete n.task; });
      App.commit();
    },
    marker(m) { const nodes = App.selectedNodes(); if (!nodes.length) return; nodes.forEach((n) => M.toggleMarker(n, m)); App.commit(); },
    style(patch) {
      const nodes = App.selectedNodes();
      nodes.forEach((n) => {
        n.style = Object.assign({}, n.style || {}, patch);
        for (const k of Object.keys(n.style)) if (n.style[k] == null || n.style[k] === '') delete n.style[k];
        if (!Object.keys(n.style).length) delete n.style;
      });
      if (nodes.length) App.commit({ noScroll: true });
    },
    toggleStyle(key) {
      const n = App.primaryNode();
      if (!n) return;
      const L = App.L.nodes.get(n.id);
      const cur = L ? L.box.style[key] : false;
      App.commands.style({ [key]: !cur });
    },
    focus() {
      if (App.focusId) { App.focusId = null; }
      else {
        const id = App.primary();
        if (!id || App.isRoot(id) || M.find(App.doc, id).floating) return toast('Select a branch to focus on.');
        App.focusId = id;
      }
      $('#focus-bar').hidden = !App.focusId;
      if (App.focusId) $('#focus-name').textContent = M.getNode(App.doc, App.focusId).text || 'Topic';
      render();
      App.fit();
    },
    zoomIn() { App.zoomAt(1.2); },
    zoomOut() { App.zoomAt(1 / 1.2); },
    actualSize() { const r = stageRect(), v = App.view; const cx = (r.width / 2 - v.x) / v.zoom, cy = (r.height / 2 - v.y) / v.zoom; animateView({ zoom: 1, x: r.width / 2 - cx, y: r.height / 2 - cy }); },
    fit() { App.fit(); },
    centerRoot() { App.centerOn(App.doc.root.id); },
    toggleOutline() { document.body.classList.toggle('no-outline'); P.store.set('hideOutline', document.body.classList.contains('no-outline')); requestAnimationFrame(applyView); },
    toggleInspector() { document.body.classList.toggle('no-inspector'); P.store.set('hideInspector', document.body.classList.contains('no-inspector')); requestAnimationFrame(applyView); },
    toggleMinimap() { document.body.classList.toggle('no-minimap'); P.store.set('showMinimap', !document.body.classList.contains('no-minimap')); App.emit('view'); },
    toggleUiTheme() {
      const cur = P.store.get('ui') || 'auto';
      const next = cur === 'auto' ? 'dark' : cur === 'dark' ? 'light' : 'auto';
      P.store.set('ui', next);
      applyUiTheme();
      toast(`Interface: ${next === 'auto' ? 'follow system' : next}`);
    },
    find() {
      const bar = $('#searchbar');
      bar.hidden = false;
      const inp = $('#search-input');
      inp.focus();
      inp.select();
    },
    openLink() { const n = App.primaryNode(); if (n && n.link) P.openExternal(n.link); },
  });

  function applyUiTheme() {
    const pref = P.store.get('ui') || 'auto';
    const dark = pref === 'dark' || (pref === 'auto' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.dataset.ui = dark ? 'dark' : 'light';
  }
  App.applyUiTheme = applyUiTheme;

  // ---------- search ----------
  function runSearch() {
    const q = $('#search-input').value;
    App.matches = M.search(App.doc, q);
    App.matchIdx = App.matches.length ? 0 : -1;
    for (const id of App.matches) M.expandTo(App.doc, id);
    $('#search-count').textContent = q ? (App.matches.length ? `1/${App.matches.length}` : 'none') : '';
    if (App.focusId && App.matches.some((id) => !M.isAncestor(App.doc, App.focusId, id) && id !== App.focusId)) { App.focusId = null; $('#focus-bar').hidden = true; }
    render();
    if (App.matchIdx >= 0) { App.sel = [App.matches[0]]; render(); App.centerOn(App.matches[0]); }
  }
  function stepSearch(d) {
    if (!App.matches.length) return;
    App.matchIdx = (App.matchIdx + d + App.matches.length) % App.matches.length;
    $('#search-count').textContent = `${App.matchIdx + 1}/${App.matches.length}`;
    App.sel = [App.matches[App.matchIdx]];
    render();
    App.centerOn(App.sel[0]);
  }
  function closeSearch() {
    $('#searchbar').hidden = true;
    App.matches = [];
    render();
    stage.focus({ preventScroll: true });
  }

  // ---------- spatial keyboard navigation ----------
  function navigate(dir) {
    const id = App.primary();
    const cur = id && App.L.nodes.get(id);
    if (!cur) return App.select(App.focusId || App.doc.root.id);
    const cx = cur.x + cur.w / 2, cy = cur.y + cur.h / 2;
    let best = null, bestScore = Infinity;
    for (const t of App.L.list) {
      if (t === cur) continue;
      const dx = t.x + t.w / 2 - cx, dy = t.y + t.h / 2 - cy;
      let main, cross;
      if (dir === 'right') { main = t.x - (cur.x + cur.w) >= -cur.w / 2 ? dx : -1; cross = dy; }
      else if (dir === 'left') { main = cur.x - (t.x + t.w) >= -cur.w / 2 ? -dx : -1; cross = dy; }
      else if (dir === 'down') { main = dy; cross = dx; }
      else { main = -dy; cross = dx; }
      if (main <= 2) continue;
      // prefer structural neighbours: parent/children/siblings get a bonus
      const related = t.parent === cur || cur.parent === t || (t.parent && t.parent === cur.parent);
      const score = main + Math.abs(cross) * 2.2 - (related ? 30 : 0);
      if (score < bestScore) { bestScore = score; best = t; }
    }
    if (best) App.select(best.id);
  }

  // ---------- keyboard ----------
  function isTextInput(el) {
    return el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
  }
  App.isTextInput = isTextInput;

  function onKeyDown(e) {
    if (document.querySelector('#modal-root .overlay')) {
      if (e.key === 'Escape') App.emit('escape-modal');
      return;
    }
    const mod = e.metaKey || e.ctrlKey;
    const key = e.key;
    if (App.present) {
      if (['ArrowRight', ' ', 'PageDown', 'Enter'].includes(key)) { App.run('presentNext'); e.preventDefault(); }
      else if (['ArrowLeft', 'PageUp', 'Backspace'].includes(key)) { App.run('presentPrev'); e.preventDefault(); }
      else if (key === 'Escape') { App.run('presentExit'); e.preventDefault(); }
      return;
    }
    if (e.target === editor) return; // handled by editor
    if (isTextInput(e.target)) {
      if (key === 'Escape') e.target.blur();
      if (mod && key.toLowerCase() === 's') { e.preventDefault(); App.run(e.shiftKey ? 'saveAs' : 'save'); }
      return;
    }
    const k = key.length === 1 ? key.toLowerCase() : key;
    const handled = (cmd, arg) => { e.preventDefault(); App.run(cmd, arg); };

    if (mod) {
      // in the desktop app the Edit menu owns undo/redo so text fields keep native undo
      if (k === 'z' && !P.isElectron) return handled(e.shiftKey ? 'redo' : 'undo');
      if (k === 'y' && !P.isElectron) return handled('redo');
      if (k === 's') return handled(e.shiftKey ? 'saveAs' : 'save');
      if (k === 'o') return handled('open');
      if (k === 'n' && !e.shiftKey) return handled('templates');
      if (k === 'f' && !e.shiftKey) return handled('find');
      if (k === 'f' && e.shiftKey) return handled('focus');
      if (k === '=' || k === '+') return handled('zoomIn');
      if (k === '-' || k === '_') return handled('zoomOut');
      if (k === '0' || e.code === 'Digit0') return handled('fit');
      if (e.altKey && (e.code === 'KeyC' || e.code === 'KeyV')) return handled(e.code === 'KeyC' ? 'copyStyle' : 'pasteStyle');
      if (k === 'd') return handled('duplicate');
      if (k === 'b' && !e.shiftKey) return handled('toggleStyle', 'bold');
      if (k === 'b' && e.shiftKey) return handled('boundary');
      if (k === 'i' && !e.shiftKey && !e.altKey) return handled('toggleStyle', 'italic');
      if (k === 'u') return handled('toggleStyle', 'underline');
      if (k === 'k') return handled('link');
      if (k === 'm' && e.shiftKey) return handled('note');
      if (k === 'r' && e.shiftKey) return handled('relation');
      if (k === 'e') return handled('exportMenu');
      if (k === 't' && e.shiftKey) return handled('task');
      if (k === '/' || e.code === 'Slash') return handled('toggleCollapse');
      if (e.code === 'Backslash') return handled(e.shiftKey ? 'toggleInspector' : 'toggleOutline');
      if (key === 'Enter') return handled('addParent');
      if (/^[1-6]$/.test(k) && e.code.startsWith('Digit')) return handled('marker', 'p' + k);
      if (key === 'ArrowUp') return handled('moveUp');
      if (key === 'ArrowDown') return handled('moveDown');
      return;
    }
    if (e.altKey && e.shiftKey && key.startsWith('Arrow')) return handled('nudge', key.slice(5).toLowerCase());
    if (e.altKey) {
      if (key === 'ArrowUp') return handled('moveUp');
      if (key === 'ArrowDown') return handled('moveDown');
      if (key === 'ArrowRight') return handled('indent');
      if (key === 'ArrowLeft') return handled('outdent');
      return;
    }
    switch (key) {
      case 'Tab': return handled(e.shiftKey ? 'addParent' : 'addChild');
      case 'Insert': return handled('addChild');
      case 'Enter': return handled('addSibling', e.shiftKey ? true : undefined);
      case 'Delete': case 'Backspace': return handled('deleteSel');
      case 'F2': return handled('edit');
      case ' ': return handled('edit');
      case 'F5': return handled('present');
      case 'F1': return handled('shortcuts');
      case 'Escape':
        e.preventDefault();
        if (drag) return App.cancelDrag();
        if (App.mode) return exitMode();
        if (!$('#searchbar').hidden) return closeSearch();
        if (App.selRel) { App.selRel = null; return render(); }
        if (App.focusId) return App.run('focus');
        return;
      case 'ArrowRight': return (e.preventDefault(), navigate('right'));
      case 'ArrowLeft': return (e.preventDefault(), navigate('left'));
      case 'ArrowUp': return (e.preventDefault(), navigate('up'));
      case 'ArrowDown': return (e.preventDefault(), navigate('down'));
      case 'Home': return (e.preventDefault(), App.select(App.focusId || App.doc.root.id), App.centerOn(App.primary()));
      default:
        // start typing on a selected topic → replace its text
        if (key.length === 1 && App.sel.length === 1 && !e.isComposing) App.startEdit(App.primary(), { initial: '' });
    }
  }

  function onEditorKey(e) {
    if (e.isComposing) return;
    if (e.key === 'Enter' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      App.commitEdit();
      if (e.metaKey || e.ctrlKey) return;
      return;
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      App.commitEdit();
      App.run(e.shiftKey ? 'addSibling' : 'addChild');
      return;
    }
    if (e.key === 'Escape') { e.preventDefault(); App.cancelEdit(); }
  }

  function exitMode() {
    App.mode = null;
    stage.classList.remove('relation-mode');
    $('#mode-hint').hidden = true;
  }

  // ---------- pointer interaction ----------
  let pan = null, drag = null, pinchZoom = null;

  function nodeIdFrom(el) { const g = el && el.closest && el.closest('.node'); return g ? g.getAttribute('data-id') : null; }

  function onPointerDown(e) {
    if (e.target === editor) return;
    App.settleView();
    if (App.editing) App.commitEdit();
    hideMenu();
    if (e.button === 2) return;
    stage.focus({ preventScroll: true });
    const actEl = e.target.closest && e.target.closest('[data-act]');
    const id = nodeIdFrom(e.target);
    const relEl = e.target.closest && e.target.closest('.relation');

    if (App.mode && App.mode.type === 'relation') {
      if (id && id !== App.mode.from) {
        const r = M.addRelation(App.doc, App.mode.from, id, '');
        exitMode();
        if (r) { App.selRel = r.id; App.sel = []; App.commit({ noScroll: true }); App.emit('select'); }
      } else exitMode();
      return;
    }
    if (id && actEl && e.button === 0) {
      const act = actEl.getAttribute('data-act');
      const node = M.getNode(App.doc, id);
      if (act === 'toggle') { M.setCollapsed(App.doc, id); App.sel = [id]; App.commit({ noScroll: true }); return; }
      if (act === 'task') { node.task = node.task === 'done' ? 'todo' : 'done'; App.commit({ noScroll: true }); return; }
      if (act === 'link') { P.openExternal(node.link); return; }
      if (act === 'note') { App.select(id); App.emit('show-tab', 'notes'); return; }
      if (act === 'marker') {
        const m = actEl.getAttribute('data-marker');
        const pm = /^pr(\d+)$/.exec(m);
        if (pm) { const nx = (+pm[1] + 25) % 125; node.icons = node.icons.map((x) => (x === m ? 'pr' + nx : x)); App.commit({ noScroll: true }); return; }
        App.select(id);
        App.emit('show-tab', 'markers');
        return;
      }
    }
    if (relEl && !id) {
      const rid = relEl.getAttribute('data-rel');
      if (App.selRel !== rid) { App.selRel = rid; App.sel = []; render(); App.emit('select'); }
      relDrag = { id: rid, sx: e.clientX, sy: e.clientY, moved: false };
      try { stage.setPointerCapture(e.pointerId); } catch (err) { /* ignore */ }
      return;
    }
    if (id) {
      if (e.shiftKey || e.metaKey || e.ctrlKey) { App.select(id, { toggle: true, noScroll: true }); return; }
      // pressing on a topic of a multi-selection keeps the selection (so the group can be dragged);
      // a plain click without dragging narrows it to that topic on release
      const inMulti = App.sel.includes(id) && App.sel.length > 1;
      if (!inMulti && (!App.sel.includes(id) || App.selRel)) App.select(id, { noScroll: true });
      drag = { id, sx: e.clientX, sy: e.clientY, started: false, pointerId: e.pointerId, narrowOnClick: inMulti };
      return;
    }
    // empty canvas: Shift + drag selects with a rectangle, plain drag pans
    if (e.shiftKey && e.button === 0) {
      marquee = { sx: e.clientX, sy: e.clientY, base: App.sel.slice(), box: null };
      try { stage.setPointerCapture(e.pointerId); } catch (err) { /* ignore */ }
      return;
    }
    pan = { sx: e.clientX, sy: e.clientY, vx: App.view.x, vy: App.view.y, moved: false };
  }

  function subtreeIds(id) { const n = M.getNode(App.doc, id); return n ? [...collect(n)] : []; }
  function collect(n, s = new Set()) { s.add(n.id); n.children.forEach((c) => collect(c, s)); return s; }

  // ---- dragging topics ----
  // Dropping on a topic re-parents (centre) or reorders (top/bottom edge). Dropping on empty canvas keeps the
  // topic attached to its parent and just moves it (and its branch) there — like XMind's free positioning.
  // A level-1 topic dragged across the central topic switches sides. Close to its home slot it snaps back.
  const SNAP = 10;
  let dragFrame = 0;

  function topLevelOf(ids) { return ids.filter((id) => !ids.some((o) => o !== id && M.isAncestor(App.doc, o, id))); }

  function beginDrag(e) {
    const doc = App.doc;
    drag.started = true;
    try { stage.setPointerCapture(e.pointerId); } catch (err) { /* pointer already released */ }
    const ids = App.sel.includes(drag.id) && App.sel.length > 1 ? topLevelOf(App.sel) : [drag.id];
    drag.ids = ids.filter((id) => !App.isRoot(id) && id !== App.focusId && M.find(doc, id));
    drag.exclude = new Set();
    for (const id of drag.ids) for (const sid of subtreeIds(id)) drag.exclude.add(sid);
    drag.key = App.doc.settings.layout;
    drag.orig = new Map();
    for (const id of drag.ids) {
      const f = M.find(doc, id), n = f.node;
      drag.orig.set(id, { floatingRoot: f.floating && !f.parent, x: n.x, y: n.y, offset: n.offset ? Object.assign({}, n.offset) : null, depth: f.depth + (f.floating ? 1 : 0) });
    }
    const t = App.L.nodes.get(drag.id);
    drag.t0 = { x: t.x, y: t.y, w: t.w, h: t.h };
    // where the primary topic would sit without its own manual offset
    const home = Lay.layout(doc, { theme: App.theme(), settings: doc.settings, measure, focusId: App.focusId, ignoreOffsetId: drag.id }).nodes.get(drag.id);
    drag.home = home ? { x: home.x, y: home.y, w: home.w, h: home.h } : null;
    drag.origJSON = snapshot();
    App.dragging = true;
    stage.classList.add('dragging-topic');
  }

  function canFree(id, d) {
    d = d || drag;
    const o = d && d.orig.get(id);
    if (!o) return false;
    if (o.floatingRoot) return true;
    const f = M.find(App.doc, id);
    if (f && f.floating) return true; // inside a floating tree: uses the logic-chart rules
    return Lay.canFreePosition(d.key, o.depth);
  }

  function applyDragDelta(dx, dy) {
    const doc = App.doc;
    drag.snapped = false;
    for (const id of drag.ids) {
      const o = drag.orig.get(id), n = M.getNode(doc, id);
      if (!n) continue;
      if (o.floatingRoot) { n.x = Math.round((o.x || 0) + dx); n.y = Math.round((o.y || 0) + dy); continue; }
      if (!canFree(id)) continue;
      const f = M.find(doc, id);
      const key = f.floating ? 'float' : drag.key;
      const base = o.offset && o.offset.l === key ? o.offset : { x: 0, y: 0 };
      let ox = base.x + dx, oy = base.y + dy;
      if (id === drag.id && drag.home && drag.ids.length === 1) {
        // distance between where the topic would land and its home slot
        const lx = drag.t0.x + dx - drag.home.x, ly = drag.t0.y + dy - drag.home.y;
        if (Math.abs(lx) < SNAP && Math.abs(ly) < SNAP) { ox -= lx; oy -= ly; drag.snapped = true; }
      }
      if (Math.abs(ox) < 0.5 && Math.abs(oy) < 0.5) delete n.offset;
      else n.offset = { x: Math.round(ox), y: Math.round(oy), l: key };
    }
  }

  function onPointerMove(e) {
    if (pan) {
      const dx = e.clientX - pan.sx, dy = e.clientY - pan.sy;
      if (!pan.moved && Math.abs(dx) + Math.abs(dy) > 3) {
        pan.moved = true;
        try { stage.setPointerCapture(e.pointerId); } catch (err) { /* pointer already released */ }
        stage.classList.add('panning');
      }
      if (!pan.moved) return;
      App.view.x = pan.vx + dx; App.view.y = pan.vy + dy;
      applyView();
      return;
    }
    if (marquee) { updateMarquee(e); return; }
    if (relDrag) { updateRelDrag(e); return; }
    if (!drag) return;
    if (!drag.started) {
      if (Math.hypot(e.clientX - drag.sx, e.clientY - drag.sy) < 5) return;
      if (App.isRoot(drag.id) || drag.id === App.focusId) { drag = null; return; }
      beginDrag(e);
      if (!drag.ids.length) { drag = null; App.dragging = false; return; }
    }
    drag.cx = e.clientX; drag.cy = e.clientY;
    if (!dragFrame) dragFrame = requestAnimationFrame(dragStep);
  }

  function dragStep() {
    dragFrame = 0;
    if (!drag || !drag.started) return;
    const w = toWorld(drag.cx, drag.cy), start = toWorld(drag.sx, drag.sy);
    const dx = w.x - start.x, dy = w.y - start.y;
    drag.world = w; drag.dx = dx; drag.dy = dy;
    // find a drop target using the positions from before this frame's move
    drag.drop = findDrop(w);
    drag.flip = !drag.drop ? sideFlip(w) : null;
    applyDragDelta(dx, dy);
    render();
    for (const id of drag.exclude) { const g = vp.querySelector(`.node[data-id="${id}"]`); if (g) g.classList.add('dragging'); }
    drawDragHints();
  }

  /** Level-1 topic of the mind map dragged to the other side of the central topic. */
  function sideFlip(w) {
    if (drag.key !== 'mindmap' || App.focusId || drag.ids.length !== 1) return null;
    const f = M.find(App.doc, drag.id);
    if (!f || f.floating || f.depth !== 1) return null;
    const r = App.L.nodes.get(App.doc.root.id);
    const cur = (App.L.nodes.get(drag.id) || {}).side || f.node.side;
    const side = w.x < r.x + r.w / 2 ? 'left' : 'right';
    return side !== cur ? side : null;
  }

  function findDrop(w) {
    const lay = App.doc.settings.layout;
    let best = null;
    for (const t of App.L.list) {
      if (drag.exclude.has(t.id)) continue;
      if (w.x < t.x - 6 || w.x > t.x + t.w + 6 || w.y < t.y - 6 || w.y > t.y + t.h + 6) continue;
      // the smallest box under the pointer wins (topics can overlap when hand-placed)
      if (best && best.w * best.h < t.w * t.h) continue;
      best = t;
    }
    if (!best) return null;
    const t = best;
    const f = M.find(App.doc, t.id);
    if (!f.parent || t.id === App.focusId) {
      const side = lay === 'mindmap' && t.id === App.doc.root.id && !App.focusId ? (w.x < t.x + t.w / 2 ? 'left' : 'right') : null;
      return { type: 'child', t, side };
    }
    const vertical = t.dir === 'd' || t.dir === 'u' || (t.depth === 1 && (lay === 'down' || lay === 'up' || lay === 'timeline'));
    const rel = vertical ? (w.x - t.x) / t.w : (w.y - t.y) / t.h;
    if (rel < 0.25) return { type: 'before', t, vertical };
    if (rel > 0.75) return { type: 'after', t, vertical };
    return { type: 'child', t };
  }

  function drawDragHints() {
    const c = '#3b82f6';
    let h = '';
    const d = drag.drop;
    if (drag.home && !d && drag.ids.length === 1 && canFree(drag.id) && !drag.orig.get(drag.id).floatingRoot) {
      const hm = drag.home;
      h += `<rect x="${hm.x - 3}" y="${hm.y - 3}" width="${hm.w + 6}" height="${hm.h + 6}" rx="9" fill="none" stroke="${c}" stroke-opacity="${drag.snapped ? 0.9 : 0.35}" stroke-width="${drag.snapped ? 2.5 : 1.5}" stroke-dasharray="5 4"/>`;
    }
    if (d) {
      const t = d.t;
      if (d.type === 'child') h += `<rect x="${t.x - 6}" y="${t.y - 6}" width="${t.w + 12}" height="${t.h + 12}" rx="10" fill="${c}" fill-opacity="0.12" stroke="${c}" stroke-width="2.5" stroke-dasharray="6 4"/>`;
      else if (d.vertical) { const x = d.type === 'before' ? t.x - 8 : t.x + t.w + 8; h += `<path d="M${x} ${t.y - 4} V${t.y + t.h + 4}" stroke="${c}" stroke-width="4" stroke-linecap="round"/>`; }
      else { const y = d.type === 'before' ? t.y - 7 : t.y + t.h + 7; h += `<path d="M${t.x - 4} ${y} H${t.x + t.w + 4}" stroke="${c}" stroke-width="4" stroke-linecap="round"/>`; }
    } else if (drag.flip) {
      const r = App.L.nodes.get(App.doc.root.id);
      const x = drag.flip === 'left' ? r.x - 14 : r.x + r.w + 14;
      h += `<path d="M${x} ${r.y - 10} V${r.y + r.h + 10}" stroke="${c}" stroke-width="4" stroke-linecap="round"/>`;
    }
    dragLayer.innerHTML = `<g class="drop-hint">${h}</g>`;
  }

  function endDrag(cancel) {
    const d = drag;
    drag = null;
    cancelAnimationFrame(dragFrame); dragFrame = 0;
    App.dragging = false;
    stage.classList.remove('dragging-topic');
    dragLayer.innerHTML = '';
    if (!d || !d.started) return;
    if (cancel) { App.doc = JSON.parse(d.origJSON); render(); return; }
    // run the last frame so the drop uses the final pointer position
    if (d.cx != null) { drag = d; dragStep(); drag = null; dragLayer.innerHTML = ''; App.dragging = false; }
    const doc = App.doc;
    const restoreOffsets = () => {
      for (const id of d.ids) {
        const o = d.orig.get(id), n = M.getNode(doc, id);
        if (!n) continue;
        if (o.floatingRoot) { n.x = o.x; n.y = o.y; }
        if (o.offset) n.offset = Object.assign({}, o.offset); else delete n.offset;
      }
    };
    let changed = false;
    if (d.drop) {
      restoreOffsets();
      const t = d.drop.t;
      if (d.drop.type === 'child') {
        for (const id of d.ids) {
          if (M.move(doc, id, t.id)) {
            changed = true;
            const n = M.getNode(doc, id);
            if (d.drop.side) n.side = d.drop.side; else delete n.side;
          }
        }
      } else {
        for (const id of (d.drop.type === 'after' ? d.ids.slice().reverse() : d.ids)) {
          const f = M.find(doc, id), tf = M.find(doc, t.id);
          if (!tf || !tf.parent) break;
          let idx = tf.index + (d.drop.type === 'after' ? 1 : 0);
          if (f.parent && f.parent.id === tf.parent.id && f.index < idx) idx--;
          if (M.move(doc, id, tf.parent.id, idx)) {
            changed = true;
            const n = M.getNode(doc, id);
            if (tf.node.side) n.side = tf.node.side; else delete n.side;
          }
        }
      }
    } else if (d.flip) {
      restoreOffsets();
      const n = M.getNode(doc, d.id);
      // place it among the topics of its new side by height
      const kids = doc.root.children.filter((c) => c !== n);
      const visual = (side) => {
        const arr = kids.filter((c) => (c.side || 'right') === side).map((c) => ({ c, t: App.L.nodes.get(c.id) }));
        return side === 'left' ? arr.reverse() : arr; // left side is stacked bottom-up
      };
      const rightV = visual('right'), leftV = visual('left');
      const target = d.flip === 'left' ? leftV : rightV;
      let at = target.findIndex((v) => v.t && d.world.y < v.t.y + v.t.h / 2);
      if (at < 0) at = target.length;
      target.splice(at, 0, { c: n });
      n.side = d.flip;
      delete n.offset;
      doc.root.children = rightV.map((v) => v.c).concat(leftV.reverse().map((v) => v.c));
      changed = true;
    } else {
      const free = d.ids.some((id) => canFree(id, d));
      if (!free) {
        restoreOffsets();
        const name = (ML.themes.LAYOUTS.find((l) => l.id === d.key) || {}).name || d.key;
        toast(d.key === 'timeline' ? 'Timeline topics stay on the axis. Drop onto a topic to move it, or drag a subtopic freely.' : `Free positioning isn't available in ${name}. Drop onto a topic to move it there.`, 3200);
      }
      changed = snapshot() !== d.origJSON;
    }
    if (changed) App.commit({ noScroll: true }); else render();
    App.emit('select');
  }

  // ---- marquee selection (Shift + drag on empty canvas) ----
  let marquee = null;
  function updateMarquee(e) {
    const a = toWorld(marquee.sx, marquee.sy), b = toWorld(e.clientX, e.clientY);
    marquee.box = { x1: Math.min(a.x, b.x), y1: Math.min(a.y, b.y), x2: Math.max(a.x, b.x), y2: Math.max(a.y, b.y) };
    const m = marquee.box;
    dragLayer.innerHTML = `<rect x="${m.x1}" y="${m.y1}" width="${m.x2 - m.x1}" height="${m.y2 - m.y1}" fill="#3b82f6" fill-opacity="0.08" stroke="#3b82f6" stroke-width="${1.2 / App.view.zoom}" stroke-dasharray="${4 / App.view.zoom} ${3 / App.view.zoom}"/>`;
  }
  function endMarquee() {
    const m = marquee.box;
    const base = marquee.base;
    marquee = null;
    dragLayer.innerHTML = '';
    if (!m) return;
    const hit = App.L.list.filter((t) => t.x < m.x2 && t.x + t.w > m.x1 && t.y < m.y2 && t.y + t.h > m.y1).map((t) => t.id);
    App.select([...new Set(base.concat(hit))], { noScroll: true });
  }

  // ---- reshaping relationship curves ----
  let relDrag = null;
  function updateRelDrag(e) {
    const r = App.doc.relations.find((x) => x.id === relDrag.id);
    const a = App.L.nodes.get(r && r.from), b = App.L.nodes.get(r && r.to);
    if (!r || !a || !b) return;
    if (!relDrag.moved && Math.hypot(e.clientX - relDrag.sx, e.clientY - relDrag.sy) < 4) return;
    relDrag.moved = true;
    const w = toWorld(e.clientX, e.clientY);
    const ax = a.x + a.w / 2, ay = a.y + a.h / 2, bx = b.x + b.w / 2, by = b.y + b.h / 2;
    const dx = bx - ax, dy = by - ay, len = Math.hypot(dx, dy) || 1;
    const nx = -dy / len, ny = dx / len;
    // the curve's middle sits 0.75 × bend away from the chord
    r.bend = Math.round(((w.x - (ax + bx) / 2) * nx + (w.y - (ay + by) / 2) * ny) / 0.75);
    if (!dragFrame) dragFrame = requestAnimationFrame(() => { dragFrame = 0; render(); });
  }

  function onPointerUp(e) {
    stage.classList.remove('panning');
    if (pan) {
      if (!pan.moved && !e.shiftKey) { App.sel = []; App.selRel = null; render(); App.emit('select'); }
      pan = null;
      return;
    }
    if (marquee) { endMarquee(); return; }
    if (relDrag) {
      const moved = relDrag.moved;
      relDrag = null;
      cancelAnimationFrame(dragFrame); dragFrame = 0;
      if (moved) App.commit({ noScroll: true });
      return;
    }
    if (drag && !drag.started) { const d = drag; drag = null; if (d.narrowOnClick) App.select(d.id, { noScroll: true }); return; }
    if (drag) endDrag(false);
  }
  App.cancelDrag = () => { if (drag && drag.started) endDrag(true); else drag = null; };

  function onWheel(e) {
    e.preventDefault();
    App.settleView();
    if (App.editing) App.commitEdit();
    const r = stageRect();
    if (e.ctrlKey || e.metaKey) {
      const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0035));
      App.zoomAt(factor, e.clientX - r.left, e.clientY - r.top);
    } else {
      const k = e.deltaMode === 1 ? 16 : 1;
      App.view.x -= (e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX) * k;
      App.view.y -= (e.shiftKey && !e.deltaX ? 0 : e.deltaY) * k;
      applyView();
    }
  }

  function onDblClick(e) {
    if (e.target === editor) return;
    const id = nodeIdFrom(e.target);
    if (id) { if (!e.target.closest('[data-act]')) App.startEdit(id); return; }
    const relEl = e.target.closest && e.target.closest('.relation');
    if (relEl) { App.selRel = relEl.getAttribute('data-rel'); App.run('relationLabel'); return; }
    App.run('floatingAt', toWorld(e.clientX, e.clientY));
  }

  // ---------- context menu ----------
  const menuEl = () => $('#ctxmenu');
  function hideMenu() { const m = menuEl(); if (m) m.hidden = true; }
  App.hideMenu = hideMenu;
  App.showMenu = function (items, x, y) {
    const m = menuEl();
    m.innerHTML = items.map((it, i) => (it === '-' ? '<div class="sep"></div>' : `<div class="mi${it.disabled ? ' disabled' : ''}" data-i="${i}">${it.icon ? ML.icons.icon(it.icon) : ''}<span>${it.label}</span>${it.kb ? `<span class="kb">${it.kb}</span>` : ''}</div>`)).join('');
    m.hidden = false;
    const r = m.getBoundingClientRect();
    m.style.left = Math.min(x, window.innerWidth - r.width - 8) + 'px';
    m.style.top = Math.min(y, window.innerHeight - r.height - 8) + 'px';
    m.onclick = (e) => {
      const el = e.target.closest('.mi');
      if (!el) return;
      hideMenu();
      const it = items[+el.getAttribute('data-i')];
      if (it && it.run) it.run();
    };
  };

  const K = (s) => (P.os === 'darwin' ? s.replace(/Mod\+/g, '⌘').replace(/Shift\+/g, '⇧').replace(/Alt\+/g, '⌥') : s.replace(/Mod\+/g, 'Ctrl+'));
  App.K = K;

  function onContextMenu(e) {
    e.preventDefault();
    if (App.editing) App.commitEdit();
    const id = nodeIdFrom(e.target);
    const relEl = e.target.closest && e.target.closest('.relation');
    const run = (c, a) => () => App.run(c, a);
    if (relEl && !id) {
      App.selRel = relEl.getAttribute('data-rel'); App.sel = []; render(); App.emit('select');
      return App.showMenu([
        { label: 'Edit label…', run: run('relationLabel') },
        { label: 'Reverse direction', run: run('relationReverse') },
        '-',
        { label: 'Delete relationship', kb: 'Del', run: run('deleteSel'), icon: 'trash' },
      ], e.clientX, e.clientY);
    }
    if (id) {
      if (!App.sel.includes(id)) App.select(id, { noScroll: true });
      const n = M.getNode(App.doc, id);
      const f = M.find(App.doc, id);
      const isRoot = App.isRoot(id);
      return App.showMenu([
        { label: 'Add subtopic', kb: 'Tab', run: run('addChild'), icon: 'subtopic' },
        { label: 'Add topic', kb: 'Enter', run: run('addSibling'), icon: 'topic' },
        { label: 'Add parent topic', kb: K('Mod+Enter'), run: run('addParent'), disabled: isRoot || !f.parent },
        { label: 'Edit text', kb: 'F2', run: run('edit') },
        '-',
        { label: n.collapsed ? 'Expand branch' : 'Collapse branch', kb: K('Mod+/'), run: run('toggleCollapse'), disabled: !n.children.length },
        { label: 'Focus on this branch', kb: K('Mod+Shift+F'), run: run('focus'), disabled: isRoot || !f.parent, icon: 'focus' },
        { label: 'Sort subtopics A→Z', run: run('sortChildren'), disabled: n.children.length < 2 },
        '-',
        { label: 'Relationship…', kb: K('Mod+Shift+R'), run: run('relation'), icon: 'relation' },
        { label: n.boundary ? 'Remove boundary' : 'Add boundary', kb: K('Mod+Shift+B'), run: run('boundary'), icon: 'boundary' },
        { label: 'Note…', kb: K('Mod+Shift+M'), run: run('note'), icon: 'note' },
        { label: 'Link…', kb: K('Mod+K'), run: run('link'), icon: 'link' },
        { label: n.image ? 'Replace image…' : 'Insert image…', run: run('image'), icon: 'image' },
        { label: n.task ? 'Cycle task state' : 'Make it a task', kb: K('Mod+Shift+T'), run: run('task') },
        '-',
        { label: 'Expand with AI…', run: () => App.run('ai', 'expand'), icon: 'ai' },
        '-',
        { label: 'Copy', kb: K('Mod+C'), run: run('copy'), icon: 'copy' },
        { label: 'Paste as subtopic', kb: K('Mod+V'), run: run('pasteMenu') },
        { label: 'Copy style', kb: K('Mod+Alt+C'), run: run('copyStyle') },
        { label: 'Paste style', kb: K('Mod+Alt+V'), run: run('pasteStyle'), disabled: !App.styleClipboard },
        { label: 'Duplicate', kb: K('Mod+D'), run: run('duplicate'), disabled: isRoot || !f.parent },
        { label: 'Detach as floating topic', run: run('detach'), disabled: isRoot || !f.parent },
        { label: 'Reset position', run: run('resetPosition'), disabled: !n.offset },
        '-',
        { label: 'Delete', kb: 'Del', run: run('deleteSel'), disabled: isRoot, icon: 'trash' },
      ], e.clientX, e.clientY);
    }
    const pt = toWorld(e.clientX, e.clientY);
    App.showMenu([
      { label: 'New floating topic here', run: () => App.run('floatingAt', pt) },
      { label: 'Paste', kb: K('Mod+V'), run: run('pasteMenu') },
      '-',
      { label: 'Fit map to window', kb: K('Mod+0'), run: run('fit'), icon: 'fit' },
      { label: 'Center on central topic', kb: 'Home', run: run('centerRoot') },
      { label: 'Reset all hand-placed topics', run: run('resetAllPositions') },
      '-',
      { label: 'Expand all', run: run('expandAll') },
      { label: 'Collapse all', run: run('collapseAll') },
      { label: 'Show only level 1', run: () => App.run('collapseLevel', 1) },
      { label: 'Show levels 1–2', run: () => App.run('collapseLevel', 2) },
      '-',
      { label: 'Map style & theme…', run: run('mapPanel'), icon: 'palette' },
    ], e.clientX, e.clientY);
  }

  // ---------- clipboard ----------
  function topSelected() {
    // drop nodes whose ancestor is also selected
    const ids = App.sel.filter((id) => !App.sel.some((o) => o !== id && M.isAncestor(App.doc, o, id)));
    return ids.map((id) => M.getNode(App.doc, id)).filter(Boolean);
  }
  function subtreeText(nodes) {
    const out = [];
    nodes.forEach((n) => M.walk(n, (x, p, d) => { out.push('\t'.repeat(d) + x.text.replace(/\n/g, ' ')); }));
    return out.join('\n');
  }
  Object.assign(App.commands, {
    copy() { const nodes = topSelected(); if (!nodes.length) return; App.clipboard = nodes.map((n) => M.clone(n)); writeClipboard(subtreeText(nodes)); toast(`Copied ${nodes.length} topic${nodes.length > 1 ? 's' : ''}`, 1200); },
    cut() { const nodes = topSelected().filter((n) => !App.isRoot(n.id)); if (!nodes.length) return; App.clipboard = nodes.map((n) => M.clone(n)); writeClipboard(subtreeText(nodes)); App.sel = nodes.map((n) => n.id); App.commands.deleteSel(); },
    paste(text) {
      const target = App.primary() || App.doc.root.id;
      let added = [];
      if (typeof text === 'string' && text.trim() && !(App.clipboard && text === subtreeText(App.clipboard))) {
        const roots = IO.parseIndented(text);
        added = roots.map((r) => M.insertSubtree(App.doc, target, r));
      } else if (App.clipboard) {
        added = App.clipboard.map((n) => M.insertSubtree(App.doc, target, n));
      }
      added = added.filter(Boolean);
      if (added.length) { App.sel = added.map((n) => n.id); App.commit(); }
    },
  });
  Object.assign(App.commands, {
    /** Paste from a menu click: use the system clipboard (text copied in any app), else topics copied here. */
    async pasteMenu() {
      let text = '';
      try { text = await navigator.clipboard.readText(); } catch (e) { /* clipboard not readable */ }
      if (!text && !App.clipboard) return toast('The clipboard is empty.');
      App.commands.paste(text);
    },
    copyStyle() {
      const n = App.primaryNode();
      if (!n) return;
      App.styleClipboard = M.clone({ style: n.style || null, boundary: !!n.boundary });
      toast('Style copied — select topics and choose Paste style', 1800);
    },
    pasteStyle() {
      const c = App.styleClipboard;
      if (!c) return toast('Copy a topic\'s style first.');
      const nodes = App.selectedNodes();
      nodes.forEach((n) => { if (c.style) n.style = M.clone(c.style); else delete n.style; });
      if (nodes.length) App.commit({ noScroll: true });
    },
  });

  function writeClipboard(text) {
    try { navigator.clipboard.writeText(text).catch(() => {}); } catch (e) { /* ignore */ }
  }
  function onCopy(e) {
    if (isTextInput(document.activeElement) || !App.sel.length) return;
    e.preventDefault();
    const nodes = topSelected();
    App.clipboard = nodes.map((n) => M.clone(n));
    e.clipboardData.setData('text/plain', subtreeText(nodes));
    toast(`Copied ${nodes.length} topic${nodes.length > 1 ? 's' : ''}`, 1200);
  }
  function onCut(e) {
    if (isTextInput(document.activeElement) || !App.sel.length) return;
    onCopy(e);
    App.commands.deleteSel();
  }
  function onPaste(e) {
    if (isTextInput(document.activeElement)) return;
    e.preventDefault();
    const items = [...(e.clipboardData.items || [])];
    const img = items.find((it) => it.kind === 'file' && it.type.startsWith('image/'));
    if (img) { App.setImageFromFile(img.getAsFile()); return; }
    App.commands.paste(e.clipboardData.getData('text/plain'));
  }

  // ---------- images ----------
  App.setImageFromFile = async function (file, nodeId) {
    const id = nodeId || App.primary();
    if (!id || !file) return;
    try {
      const src = await downscaleImage(file, 900);
      const img = await loadImg(src);
      const node = M.getNode(App.doc, id);
      const w = Math.min(240, img.naturalWidth);
      node.image = { src, w, h: Math.round((img.naturalHeight * w) / img.naturalWidth) };
      App.sel = [id];
      App.commit();
    } catch (err) { toast('Could not load that image.'); }
  };
  function loadImg(src) { return new Promise((res, rej) => { const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = src; }); }
  async function downscaleImage(file, max) {
    const url = URL.createObjectURL(file);
    try {
      const img = await loadImg(url);
      const k = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
      const c = document.createElement('canvas');
      c.width = Math.max(1, Math.round(img.naturalWidth * k)); c.height = Math.max(1, Math.round(img.naturalHeight * k));
      c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
      return c.toDataURL(file.type === 'image/png' || file.type === 'image/gif' || file.type === 'image/svg+xml' ? 'image/png' : 'image/jpeg', 0.88);
    } finally { URL.revokeObjectURL(url); }
  }

  // ---------- files ----------
  App.confirmDiscard = async function () {
    if (!App.dirty) return true;
    const choice = await App.ask('Save changes?', `“${App.name}” has unsaved changes.`, [
      { label: 'Cancel', value: 'cancel' }, { label: "Don't save", value: 'discard', cls: 'danger' }, { label: 'Save', value: 'save', cls: 'primary' },
    ]);
    if (choice === 'save') return App.save();
    return choice === 'discard';
  };

  App.save = async function (as) {
    if (App.editing) App.commitEdit();
    const text = IO.serialize(App.doc);
    const fname = (App.name || 'Untitled').replace(/[\\/:*?"<>|]+/g, '-') + '.mindloom';
    try {
      const r = await P.save(text, as ? null : App.path, fname);
      if (!r) return false;
      if (r.path) { App.path = r.path; App.name = r.name.replace(/\.mindloom$/i, ''); }
      setDirty(false);
      scheduleAutosave();
      $('#st-save').textContent = 'Saved ' + new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      return true;
    } catch (err) {
      toast('Save failed: ' + err.message, 4000);
      return false;
    }
  };

  App.loadFile = async function (r) {
    try {
      let doc;
      if (r.buffer) doc = await IO.fromXMind(r.buffer);
      else doc = IO.importText(r.name, r.text);
      const native = /\.mindloom$/i.test(r.name);
      const base = r.name.replace(/\.[^.]+$/, '');
      App.setDoc(doc, { path: native ? r.path : null, name: base, dirty: !native });
      if (!native) toast(`Imported ${r.name}`);
      return true;
    } catch (err) {
      toast(`Could not open ${r.name}: ${err.message}`, 4500);
      return false;
    }
  };

  Object.assign(App.commands, {
    async open() { if (!(await App.confirmDiscard())) return; const r = await P.open(); if (r) App.loadFile(r); },
    async openPath(path) {
      if (!(await App.confirmDiscard())) return;
      const r = await P.readPath(path);
      if (r && !r.error) App.loadFile(r); else toast(`Could not open ${path}`);
    },
    save() { return App.save(false); },
    saveAs() { return App.save(true); },
    async saveThenClose() { if (await App.save(false)) { setDirty(false); if (window.mindloom && window.mindloom.forceClose) window.mindloom.forceClose(); else window.close(); } },
    async newBlank() { if (!(await App.confirmDiscard())) return; App.setDoc(ML.templates.build('blank')); App.startEdit(App.doc.root.id); },
  });

  // ---------- export ----------
  function exportData(opts) {
    const theme = App.theme();
    const L = App.L;
    return R.exportSVG(L, App.doc, Object.assign({ theme, settings: App.doc.settings }, opts || {}));
  }
  App.exportData = exportData;

  App.renderPNG = async function (scale, transparent) {
    const { svg: s, width, height } = exportData({ transparent });
    const k = Math.min(scale || 2, 16000 / width, 16000 / height, Math.sqrt(120e6 / (width * height)));
    const img = await loadImg('data:image/svg+xml;charset=utf-8,' + encodeURIComponent(s));
    const c = document.createElement('canvas');
    c.width = Math.round(width * k); c.height = Math.round(height * k);
    const ctx = c.getContext('2d');
    ctx.scale(k, k);
    ctx.drawImage(img, 0, 0, width, height);
    return new Promise((res) => c.toBlob(res, 'image/png'));
  };

  const fileBase = () => (App.name || 'mind map').replace(/[\\/:*?"<>|]+/g, '-');
  Object.assign(App.commands, {
    async exportPNG(transparent) {
      const blob = await App.renderPNG(2, transparent === true);
      const r = await P.exportFile(fileBase() + '.png', blob, 'png', 'PNG Image');
      if (r) toast('Exported PNG');
    },
    async exportSVG() {
      const r = await P.exportFile(fileBase() + '.svg', '<?xml version="1.0" encoding="UTF-8"?>\n' + exportData().svg, 'svg', 'SVG Image');
      if (r) toast('Exported SVG');
    },
    async exportPDF() {
      const d = exportData();
      const r = await P.exportPDF(d.svg, d.width, d.height, fileBase() + '.pdf');
      if (r && r.path) toast('Exported PDF');
    },
    async exportMarkdown() { if (await P.exportFile(fileBase() + '.md', IO.toMarkdown(App.doc), 'md', 'Markdown')) toast('Exported Markdown'); },
    async exportOPML() { if (await P.exportFile(fileBase() + '.opml', IO.toOPML(App.doc), 'opml', 'OPML')) toast('Exported OPML'); },
    async exportText() { if (await P.exportFile(fileBase() + '.txt', IO.toText(App.doc), 'txt', 'Text')) toast('Exported text outline'); },
    async exportFreeMind() { if (await P.exportFile(fileBase() + '.mm', IO.toFreeMind(App.doc), 'mm', 'FreeMind')) toast('Exported FreeMind map'); },
    async exportJSON() { if (await P.exportFile(fileBase() + '.mindloom', IO.serialize(App.doc), 'mindloom', 'Mindloom map')) toast('Exported copy'); },
    async copyImage() {
      try {
        const blob = await App.renderPNG(2);
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
        toast('Map image copied to clipboard');
      } catch (e) { toast('Copying images is not supported here.'); }
    },
  });

  // ---------- presentation ----------
  Object.assign(App.commands, {
    present() {
      if (App.editing) App.commitEdit();
      App.focusId = null;
      $('#focus-bar').hidden = true;
      const slides = [{ id: null }].concat(App.doc.root.children.map((c) => ({ id: c.id })));
      const collapsed = [];
      M.walkDoc(App.doc, (n) => { if (n.collapsed) collapsed.push(n.id); });
      App.present = { slides, i: 0, sel: App.sel.slice(), collapsed };
      App.sel = [];
      document.body.classList.add('presenting');
      $('#present-bar').hidden = false;
      P.fullscreen(true);
      setTimeout(() => { render(); showSlide(); }, 250);
    },
    presentNext() { if (!App.present) return; App.present.i = Math.min(App.present.slides.length - 1, App.present.i + 1); showSlide(); },
    presentPrev() { if (!App.present) return; App.present.i = Math.max(0, App.present.i - 1); showSlide(); },
    presentExit() {
      if (!App.present) return;
      App.sel = App.present.sel;
      // presenting may open collapsed branches; put them back exactly as they were
      const was = new Set(App.present.collapsed);
      M.walkDoc(App.doc, (n) => { if (was.has(n.id)) n.collapsed = true; else delete n.collapsed; });
      App.present = null;
      document.body.classList.remove('presenting');
      $('#present-bar').hidden = true;
      P.fullscreen(false);
      setTimeout(() => { render(); App.fit(); }, 250);
    },
  });
  function showSlide() {
    const p = App.present;
    const s = p.slides[p.i];
    $('#present-count').textContent = `${p.i + 1} / ${p.slides.length}`;
    if (!s.id) { App.sel = []; render(); App.fitBox(App.L.bounds, true, 1.6, 50); return; }
    const n = M.getNode(App.doc, s.id);
    if (n && n.collapsed) { n.collapsed = false; }
    App.sel = [];
    render();
    const t = App.L.nodes.get(s.id);
    if (!t) return;
    const b = Lay.subtreeBox(t);
    App.fitBox({ minX: b.minX, minY: b.minY, w: b.maxX - b.minX, h: b.maxY - b.minY }, true, 2, 90);
  }

  // ---------- run command ----------
  App.run = function (name, arg) {
    const fn = App.commands[name];
    if (!fn) { console.warn('Unknown command', name); return; }
    hideMenu();
    if (App.editing && !['undo', 'redo'].includes(name)) App.commitEdit();
    try { return fn(arg); } catch (err) { console.error(err); toast('Something went wrong: ' + err.message, 4000); }
  };

  // ---------- init ----------
  App.init = function () {
    stage = $('#stage'); svg = $('#canvas'); vp = $('#viewport'); dragLayer = $('#drag-layer'); editor = $('#editor');
    stage.addEventListener('pointerdown', onPointerDown);
    stage.addEventListener('pointermove', onPointerMove);
    stage.addEventListener('pointerup', onPointerUp);
    stage.addEventListener('pointercancel', onPointerUp);
    stage.addEventListener('wheel', onWheel, { passive: false });
    stage.addEventListener('dblclick', onDblClick);
    stage.addEventListener('contextmenu', onContextMenu);
    // Safari/WebKit pinch gestures
    stage.addEventListener('gesturestart', (e) => { e.preventDefault(); pinchZoom = App.view.zoom; });
    stage.addEventListener('gesturechange', (e) => { e.preventDefault(); if (pinchZoom) { const r = stageRect(); App.zoomAt((pinchZoom * e.scale) / App.view.zoom, e.clientX - r.left, e.clientY - r.top); } });
    document.addEventListener('keydown', onKeyDown);
    editor.addEventListener('keydown', onEditorKey);
    editor.addEventListener('input', () => {
      const ed = App.editing;
      if (!ed) return;
      const node = M.getNode(App.doc, ed.id);
      if (node) { node.text = editor.value; render(); }
    });
    editor.addEventListener('blur', () => { setTimeout(() => { if (App.editing && document.activeElement !== editor) App.commitEdit(); }, 0); });
    document.addEventListener('copy', onCopy);
    document.addEventListener('cut', onCut);
    document.addEventListener('paste', onPaste);
    document.addEventListener('pointerdown', (e) => { if (!e.target.closest('#ctxmenu')) hideMenu(); }, true);
    window.addEventListener('resize', () => applyView());
    if (!P.isElectron) window.addEventListener('beforeunload', (e) => { if (App.dirty) { e.preventDefault(); e.returnValue = ''; } });
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyUiTheme);

    $('#search-input').addEventListener('input', runSearch);
    $('#search-input').addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); stepSearch(e.shiftKey ? -1 : 1); }
      if (e.key === 'Escape') { e.preventDefault(); closeSearch(); }
    });
    $('#search-next').onclick = () => stepSearch(1);
    $('#search-prev').onclick = () => stepSearch(-1);
    $('#search-close').onclick = closeSearch;

    // drag & drop files: map files open, images attach to the topic under the pointer
    window.addEventListener('dragover', (e) => { e.preventDefault(); });
    window.addEventListener('drop', async (e) => {
      e.preventDefault();
      const file = e.dataTransfer.files && e.dataTransfer.files[0];
      if (!file) return;
      if (file.type.startsWith('image/')) {
        const id = nodeIdFrom(document.elementFromPoint(e.clientX, e.clientY)) || App.primary();
        if (id) App.setImageFromFile(file, id); else toast('Drop the image onto a topic.');
        return;
      }
      if (!(await App.confirmDiscard())) return;
      if (/\.xmind$/i.test(file.name)) App.loadFile({ name: file.name, buffer: await file.arrayBuffer(), path: null });
      else App.loadFile({ name: file.name, text: await file.text(), path: P.isElectron && window.mindloom.pathForFile ? window.mindloom.pathForFile(file) : null });
    });

    // toolbar
    document.querySelectorAll('[data-icon]').forEach((b) => { b.insertAdjacentHTML('afterbegin', ML.icons.icon(b.getAttribute('data-icon'))); });
    document.addEventListener('click', (e) => {
      const b = e.target.closest('[data-cmd]');
      if (!b || b.closest('#modal-root')) return;
      App.run(b.getAttribute('data-cmd'), b.getAttribute('data-arg') || undefined);
    });
    P.onMenu((cmd, arg) => App.run(cmd, arg));
    P.onOpenPath((path) => App.run('openPath', path));

    if (P.store.get('hideOutline') !== false) document.body.classList.add('no-outline');
    if (P.store.get('hideInspector')) document.body.classList.add('no-inspector');
    if (!P.store.get('showMinimap')) document.body.classList.add('no-minimap');
    if (P.isElectron && P.os === 'darwin') document.body.classList.add('mac-electron');
    applyUiTheme();
  };
})(window.ML = window.ML || {});
