/* Mindloom — document model: tree operations, search, undo history (pure) */
(function (ML) {
  'use strict';

  let counter = 0;
  function uid() {
    counter = (counter + 1) % 1e6;
    return 'n' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7) + counter.toString(36);
  }

  function createNode(text, extra) {
    return Object.assign({ id: uid(), text: text == null ? '' : String(text), children: [] }, extra || {});
  }

  function clone(o) { return JSON.parse(JSON.stringify(o)); }

  function createDoc(title) {
    const T = ML.themes;
    return {
      app: 'mindloom', version: 1,
      root: createNode(title || 'Central Topic'),
      floating: [],
      relations: [],
      settings: clone(T.DEFAULT_SETTINGS),
    };
  }

  /** Fill in missing fields and repair duplicate/missing ids so any loaded file is safe to use. */
  function normalizeDoc(doc) {
    if (!doc || typeof doc !== 'object' || !doc.root) throw new Error('Not a Mindloom document');
    const T = ML.themes;
    doc.app = 'mindloom';
    doc.version = 1;
    doc.floating = Array.isArray(doc.floating) ? doc.floating : [];
    doc.relations = Array.isArray(doc.relations) ? doc.relations : [];
    doc.settings = Object.assign(clone(T.DEFAULT_SETTINGS), doc.settings || {});
    doc.settings.shapes = Object.assign(clone(T.DEFAULT_SETTINGS.shapes), (doc.settings && doc.settings.shapes) || {});
    const seen = new Set();
    const fix = (n) => {
      if (!n.id || seen.has(n.id)) n.id = uid();
      seen.add(n.id);
      n.text = n.text == null ? '' : String(n.text);
      n.children = Array.isArray(n.children) ? n.children : [];
      n.children.forEach(fix);
    };
    fix(doc.root);
    doc.floating.forEach((f) => { fix(f); if (f.x == null && f.y == null) f._place = true; f.x = +f.x || 0; f.y = +f.y || 0; });
    doc.relations = doc.relations.filter((r) => r && seen.has(r.from) && seen.has(r.to));
    doc.relations.forEach((r) => { if (!r.id) r.id = uid(); });
    return doc;
  }

  function trees(doc) { return [doc.root].concat(doc.floating); }

  function walk(node, fn, parent = null, depth = 0, index = 0) {
    if (fn(node, parent, depth, index) === false) return false;
    for (let i = 0; i < node.children.length; i++) {
      if (walk(node.children[i], fn, node, depth + 1, i) === false) return false;
    }
    return true;
  }

  function walkDoc(doc, fn) {
    for (const t of trees(doc)) if (walk(t, fn) === false) return;
  }

  /** Locate a node. Returns {node, parent, list, index, depth, tree, floating} or null. */
  function find(doc, id) {
    let res = null;
    for (const t of trees(doc)) {
      walk(t, (n, p, d, i) => {
        if (n.id === id) {
          const floating = t !== doc.root;
          res = { node: n, parent: p, depth: d, tree: t, floating, index: p ? i : floating ? doc.floating.indexOf(n) : 0, list: p ? p.children : floating ? doc.floating : null };
          return false;
        }
      });
      if (res) break;
    }
    return res;
  }

  function getNode(doc, id) { const f = find(doc, id); return f ? f.node : null; }

  function pathTo(doc, id) {
    for (const t of trees(doc)) {
      const path = [];
      const rec = (n) => {
        path.push(n);
        if (n.id === id) return true;
        for (const c of n.children) if (rec(c)) return true;
        path.pop();
        return false;
      };
      if (rec(t)) return path;
    }
    return [];
  }

  function isAncestor(doc, ancestorId, id) {
    return pathTo(doc, id).slice(0, -1).some((n) => n.id === ancestorId);
  }

  function addChild(doc, parentId, text, index) {
    const p = getNode(doc, parentId);
    if (!p) return null;
    const n = createNode(text == null ? '' : text);
    if (index == null || index < 0 || index > p.children.length) p.children.push(n);
    else p.children.splice(index, 0, n);
    p.collapsed = false;
    return n;
  }

  function addSibling(doc, id, text, before) {
    const f = find(doc, id);
    if (!f) return null;
    if (!f.parent) {
      if (f.floating) {
        const n = createNode(text == null ? '' : text, { x: f.node.x, y: f.node.y + 80 });
        doc.floating.push(n);
        return n;
      }
      return addChild(doc, id, text); // root has no siblings
    }
    const n = createNode(text == null ? '' : text);
    f.list.splice(before ? f.index : f.index + 1, 0, n);
    return n;
  }

  function addParent(doc, id, text) {
    const f = find(doc, id);
    if (!f || !f.parent) return null;
    const n = createNode(text == null ? '' : text);
    f.list.splice(f.index, 1, n);
    n.children.push(f.node);
    if (f.node.side) { n.side = f.node.side; delete f.node.side; }
    return n;
  }

  function collectIds(node, out = new Set()) { walk(node, (n) => { out.add(n.id); }); return out; }

  /** Remove nodes (never the main root). Returns id of a sensible node to select afterwards. */
  function remove(doc, ids) {
    let next = null;
    const removed = new Set();
    for (const id of ids) {
      const f = find(doc, id);
      if (!f || (!f.parent && !f.floating)) continue;
      if (f.parent) {
        next = (f.list[f.index + 1] || f.list[f.index - 1] || f.parent).id;
        f.list.splice(f.index, 1);
      } else {
        doc.floating.splice(f.index, 1);
        next = doc.root.id;
      }
      collectIds(f.node, removed);
    }
    if (removed.size) doc.relations = doc.relations.filter((r) => !removed.has(r.from) && !removed.has(r.to));
    if (next && removed.has(next)) next = doc.root.id;
    return next;
  }

  /** Move node under newParent at index. Refuses cycles. */
  function move(doc, id, newParentId, index) {
    if (id === newParentId || id === doc.root.id) return false;
    if (isAncestor(doc, id, newParentId)) return false;
    const f = find(doc, id);
    const np = getNode(doc, newParentId);
    if (!f || !np) return false;
    if (f.parent) f.list.splice(f.index, 1);
    else if (f.floating) { doc.floating.splice(f.index, 1); delete f.node.x; delete f.node.y; }
    else return false;
    if (index == null || index < 0 || index > np.children.length) np.children.push(f.node);
    else np.children.splice(index, 0, f.node);
    np.collapsed = false;
    return true;
  }

  function detach(doc, id, x, y) {
    const f = find(doc, id);
    if (!f || !f.parent) return false;
    f.list.splice(f.index, 1);
    f.node.x = x; f.node.y = y;
    delete f.node.side;
    doc.floating.push(f.node);
    return true;
  }

  function reorder(doc, id, delta) {
    const f = find(doc, id);
    if (!f || !f.parent) return false;
    const j = f.index + delta;
    if (j < 0 || j >= f.list.length) return false;
    f.list.splice(f.index, 1);
    f.list.splice(j, 0, f.node);
    return true;
  }

  /** Make a node a child of its previous sibling (outline "indent"). */
  function indent(doc, id) {
    const f = find(doc, id);
    if (!f || !f.parent || f.index === 0) return false;
    const prev = f.list[f.index - 1];
    f.list.splice(f.index, 1);
    prev.children.push(f.node);
    prev.collapsed = false;
    return true;
  }

  /** Move a node up one level, right after its parent (outline "outdent"). */
  function outdent(doc, id) {
    const path = pathTo(doc, id);
    if (path.length < 3) return false;
    const node = path[path.length - 1], parent = path[path.length - 2], grand = path[path.length - 3];
    parent.children.splice(parent.children.indexOf(node), 1);
    grand.children.splice(grand.children.indexOf(parent) + 1, 0, node);
    return true;
  }

  function cloneWithNewIds(node) {
    const c = clone(node);
    walk(c, (n) => { n.id = uid(); });
    return c;
  }

  function duplicate(doc, id) {
    const f = find(doc, id);
    if (!f || !f.parent) return null;
    const c = cloneWithNewIds(f.node);
    f.list.splice(f.index + 1, 0, c);
    return c;
  }

  function insertSubtree(doc, parentId, subtree, index) {
    const p = getNode(doc, parentId);
    if (!p) return null;
    const c = cloneWithNewIds(subtree);
    delete c.x; delete c.y;
    if (index == null) p.children.push(c); else p.children.splice(index, 0, c);
    p.collapsed = false;
    return c;
  }

  function expandTo(doc, id) {
    const path = pathTo(doc, id);
    path.slice(0, -1).forEach((n) => { n.collapsed = false; });
  }

  function setCollapsed(doc, id, value) {
    const n = getNode(doc, id);
    if (!n || !n.children.length) return false;
    n.collapsed = value == null ? !n.collapsed : !!value;
    return true;
  }

  function collapseToLevel(doc, level) {
    walk(doc.root, (n, p, d) => { if (n.children.length) n.collapsed = d >= level; });
  }

  function plainText(n) {
    return [n.text, n.note || '', (n.tags || []).join(' '), n.link || ''].join(' ').toLowerCase();
  }

  function search(doc, query) {
    const q = (query || '').trim().toLowerCase();
    if (!q) return [];
    const out = [];
    walkDoc(doc, (n) => { if (plainText(n).includes(q)) out.push(n.id); });
    return out;
  }

  function stats(doc) {
    let nodes = 0, words = 0, done = 0, tasks = 0;
    walkDoc(doc, (n) => {
      nodes++;
      words += (n.text.match(/\S+/g) || []).length;
      if (n.task) { tasks++; if (n.task === 'done') done++; }
    });
    return { nodes, words, tasks, done };
  }

  function toggleMarker(node, marker) {
    const group = ML.themes.markerGroupOf(marker);
    node.icons = node.icons || [];
    const had = node.icons.includes(marker);
    node.icons = node.icons.filter((m) => ML.themes.markerGroupOf(m) !== group);
    if (!had) node.icons.push(marker);
    if (!node.icons.length) delete node.icons;
  }

  function addRelation(doc, from, to, label) {
    if (from === to || !getNode(doc, from) || !getNode(doc, to)) return null;
    const r = { id: uid(), from, to, label: label || '' };
    doc.relations.push(r);
    return r;
  }

  /** Snapshot-based undo history. */
  class History {
    constructor(limit = 300) { this.limit = limit; this.stack = []; this.idx = -1; }
    reset(snapshot) { this.stack = [snapshot]; this.idx = 0; }
    push(snapshot) {
      if (this.stack[this.idx] === snapshot) return false;
      this.stack = this.stack.slice(0, this.idx + 1);
      this.stack.push(snapshot);
      if (this.stack.length > this.limit) this.stack.shift();
      this.idx = this.stack.length - 1;
      return true;
    }
    canUndo() { return this.idx > 0; }
    canRedo() { return this.idx < this.stack.length - 1; }
    undo() { return this.canUndo() ? this.stack[--this.idx] : null; }
    redo() { return this.canRedo() ? this.stack[++this.idx] : null; }
  }

  ML.model = {
    uid, createNode, createDoc, normalizeDoc, clone, trees, walk, walkDoc, find, getNode, pathTo, isAncestor,
    addChild, addSibling, addParent, remove, move, detach, reorder, indent, outdent, duplicate, insertSubtree,
    cloneWithNewIds, expandTo, setCollapsed, collapseToLevel, search, stats, toggleMarker, addRelation, History,
  };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
