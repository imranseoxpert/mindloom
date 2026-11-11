/* Mindloom — import / export of text formats (Markdown, OPML, indented text, FreeMind, XMind) */
(function (ML) {
  'use strict';
  const M = ML.model;

  // ---------- tiny XML parser (no DOM needed, works in tests) ----------
  const ENT = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };
  function decodeEntities(s) {
    return String(s).replace(/&(#x[0-9a-f]+|#\d+|\w+);/gi, (m, e) => {
      if (e[0] === '#') { const c = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10); return Number.isFinite(c) ? String.fromCodePoint(c) : m; }
      return ENT[e.toLowerCase()] != null ? ENT[e.toLowerCase()] : m;
    });
  }
  function xmlEsc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/\n/g, '&#10;'); }

  function parseXML(src) {
    const root = { name: '#root', attrs: {}, children: [], text: '' };
    const stack = [root];
    const re = /<!\[CDATA\[([\s\S]*?)\]\]>|<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<!DOCTYPE[^>]*>|<\/\s*([\w:.-]+)\s*>|<([\w:.-]+)((?:\s+[\w:.-]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(\/?)>|([^<]+)/g;
    let m;
    while ((m = re.exec(src))) {
      const top = stack[stack.length - 1];
      if (m[1] != null) top.text += m[1];
      else if (m[2]) { if (stack.length > 1) stack.pop(); }
      else if (m[3]) {
        const el = { name: m[3], attrs: {}, children: [], text: '' };
        const ar = /([\w:.-]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
        let a;
        while ((a = ar.exec(m[4] || ''))) el.attrs[a[1]] = decodeEntities(a[3] != null ? a[3] : a[4]);
        top.children.push(el);
        if (!m[5]) stack.push(el);
      } else if (m[6] != null) top.text += decodeEntities(m[6]);
    }
    return root;
  }
  const kids = (el, name) => el.children.filter((c) => c.name.toLowerCase() === name.toLowerCase());
  const first = (el, name) => kids(el, name)[0];
  function deepFirst(el, name) {
    for (const c of el.children) {
      if (c.name.toLowerCase() === name.toLowerCase()) return c;
      const d = deepFirst(c, name);
      if (d) return d;
    }
    return null;
  }

  /** Several top-level trees: flagged ones become floating topics; more than one unflagged gets a wrapper root. */
  function docFromRoots(roots, isFloating, title) {
    const main = roots.filter((r, i) => !isFloating(r, i));
    const floats = roots.filter((r, i) => isFloating(r, i));
    if (!main.length) { main.push(floats.shift()); }
    let root = main[0];
    if (main.length > 1) { root = M.createNode(title || 'Imported'); root.children = main; }
    floats.forEach((f) => { f._place = true; });
    return docFromTree(root, floats);
  }

  function docFromTree(rootNode, floating) {
    const doc = M.createDoc();
    doc.root = rootNode;
    if (floating) doc.floating = floating;
    return M.normalizeDoc(doc);
  }

  // ---------- indented plain text ----------
  function indentOf(line) {
    const m = /^[\t ]*/.exec(line)[0];
    let w = 0;
    for (const ch of m) w += ch === '\t' ? 4 : 1;
    return w;
  }

  /** Parse indented lines into an array of root-level nodes. Bullets (-, *, +, 1.) are stripped. */
  function parseIndented(text) {
    const lines = String(text).replace(/\r\n?/g, '\n').split('\n');
    const roots = [];
    roots.gaps = [];
    const stack = [];
    let gap = false;
    for (const raw of lines) {
      if (!raw.trim()) { gap = true; continue; }
      const ind = indentOf(raw);
      let t = raw.trim();
      let task = null;
      t = t.replace(/^(?:[-*+•]|\d+[.)])\s+/, '');
      const tm = /^\[([ xX])\]\s+/.exec(t);
      if (tm) { task = tm[1] === ' ' ? 'todo' : 'done'; t = t.slice(tm[0].length); }
      const node = M.createNode(t);
      if (task) node.task = task;
      while (stack.length && stack[stack.length - 1].ind >= ind) stack.pop();
      if (stack.length) stack[stack.length - 1].node.children.push(node);
      else { roots.push(node); roots.gaps.push(gap); }
      stack.push({ ind, node });
      gap = false;
    }
    return roots;
  }

  function fromText(text) {
    const roots = parseIndented(text);
    if (!roots.length) throw new Error('Nothing to import');
    // a first tree followed by blank-line separated trees = a map plus its floating topics
    const floats = roots.length > 1 && roots[0].children.length > 0 && roots.gaps.slice(1).every(Boolean);
    return docFromRoots(roots, (r, i) => floats && i > 0);
  }

  function toText(doc) {
    return M.trees(doc).map((t) => {
      const out = [];
      M.walk(t, (n, p, d) => { out.push('\t'.repeat(d) + n.text.replace(/\n/g, ' ')); });
      return out.join('\n');
    }).join('\n\n') + '\n';
  }

  // ---------- Markdown ----------
  function toMarkdown(doc) {
    const out = [];
    const emit = (tree, i) => {
      if (i > 0) out.push('<!-- floating -->');
      M.walk(tree, (n, p, d) => {
        const text = n.text.replace(/\n/g, ' ');
        const task = n.task ? (n.task === 'done' ? '[x] ' : '[ ] ') : '';
        const link = n.link ? ` ([link](${n.link}))` : '';
        if (d === 0) out.push(`# ${text}${link}`, '');
        else if (d === 1) out.push(`## ${task}${text}${link}`, '');
        else out.push(`${'  '.repeat(d - 2)}- ${task}${text}${link}`);
        if (n.note) {
          const pad = d >= 2 ? '  '.repeat(d - 1) : '';
          out.push(...n.note.split('\n').map((l) => `${pad}> ${l}`));
          if (d < 2) out.push('');
        }
        if (d === 1 && !n.children.length) { /* spacing */ }
        if (d >= 1 && n.children.length && d === 1) { /* list follows */ }
      });
      out.push('');
    };
    M.trees(doc).forEach(emit);
    return out.join('\n').replace(/\n{3,}/g, '\n\n').trim() + '\n';
  }

  function fromMarkdown(text) {
    const lines = String(text).replace(/\r\n?/g, '\n').split('\n');
    const roots = [];
    const stack = []; // {level, node}
    let lastNode = null;
    let baseLevel = 0;
    let inFence = false;
    let floatNext = false;
    const floating = new Set();
    const clean = (s) => s.replace(/\*\*(.+?)\*\*/g, '$1').replace(/__(.+?)__/g, '$1').replace(/`([^`]+)`/g, '$1').replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1').trim();
    const attach = (level, node) => {
      while (stack.length && stack[stack.length - 1].level >= level) stack.pop();
      if (floatNext) { stack.length = 0; floating.add(node); floatNext = false; }
      if (stack.length) stack[stack.length - 1].node.children.push(node);
      else roots.push(node);
      stack.push({ level, node });
      lastNode = node;
    };
    for (const raw of lines) {
      if (/^\s*```/.test(raw)) { inFence = !inFence; continue; }
      if (inFence || !raw.trim()) continue;
      if (/^\s*<!--\s*floating\s*-->\s*$/.test(raw)) { floatNext = true; continue; }
      if (/^\s*<!--.*-->\s*$/.test(raw)) continue;
      let m;
      if ((m = /^(#{1,6})\s+(.*)$/.exec(raw))) {
        const level = m[1].length * 100;
        baseLevel = level;
        attach(level, mk(m[2]));
      } else if ((m = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/.exec(raw))) {
        const level = baseLevel + 1 + indentOf(m[1]);
        attach(level, mk(m[2]));
      } else if ((m = /^\s*>\s?(.*)$/.exec(raw)) && lastNode) {
        lastNode.note = (lastNode.note ? lastNode.note + '\n' : '') + m[1];
      } else if (lastNode && !/^\s*(---|\*\*\*|___)\s*$/.test(raw)) {
        lastNode.note = (lastNode.note ? lastNode.note + '\n' : '') + raw.trim();
      }
    }
    function mk(s) {
      let task = null;
      const tm = /^\[([ xX])\]\s+/.exec(s);
      if (tm) { task = tm[1] === ' ' ? 'todo' : 'done'; s = s.slice(tm[0].length); }
      const lm = /\[[^\]]*\]\((https?:[^)]+)\)/.exec(s);
      const n = M.createNode(clean(s));
      if (task) n.task = task;
      if (lm) n.link = lm[1];
      return n;
    }
    if (!roots.length) throw new Error('No headings or list items found');
    return docFromRoots(roots, (r) => floating.has(r));
  }

  // ---------- OPML ----------
  function toOPML(doc) {
    const rec = (n, d, fl) => {
      const pad = '  '.repeat(d + 2);
      const note = (n.note ? ` _note="${xmlEsc(n.note)}"` : '') + (fl ? ' _floating="true"' : '');
      if (!n.children.length) return `${pad}<outline text="${xmlEsc(n.text)}"${note}/>\n`;
      return `${pad}<outline text="${xmlEsc(n.text)}"${note}>\n${n.children.map((c) => rec(c, d + 1)).join('')}${pad}</outline>\n`;
    };
    return `<?xml version="1.0" encoding="UTF-8"?>\n<opml version="2.0">\n  <head><title>${xmlEsc(doc.root.text)}</title></head>\n  <body>\n${M.trees(doc).map((t, i) => rec(t, 0, i > 0)).join('')}  </body>\n</opml>\n`;
  }

  function fromOPML(text) {
    const x = parseXML(text);
    const body = deepFirst(x, 'body');
    if (!body) throw new Error('Not an OPML file');
    const conv = (el) => {
      const n = M.createNode(el.attrs.text || el.attrs.title || '');
      if (el.attrs._note) n.note = el.attrs._note;
      n.children = kids(el, 'outline').map(conv);
      return n;
    };
    const els = kids(body, 'outline');
    const roots = els.map(conv);
    if (!roots.length) throw new Error('OPML has no outlines');
    const head = deepFirst(x, 'title');
    return docFromRoots(roots, (r, i) => els[i].attrs._floating === 'true', head && head.text.trim());
  }

  // ---------- FreeMind / Freeplane (.mm) ----------
  function toFreeMind(doc) {
    const rec = (n, d, side, free) => {
      const pad = '  '.repeat(d + 1);
      const pos = d === 1 ? ` POSITION="${side}"` : '';
      const link = n.link ? ` LINK="${xmlEsc(n.link)}"` : '';
      const folded = n.collapsed ? ' FOLDED="true"' : '';
      if (!n.children.length && !n.note && !free && !(d === 0 && doc.floating.length)) return `${pad}<node TEXT="${xmlEsc(n.text)}"${pos}${link}${folded}/>\n`;
      const note = n.note ? `${pad}  <richcontent TYPE="NOTE"><html><body><p>${xmlEsc(n.note)}</p></body></html></richcontent>\n` : '';
      const hook = free ? `${pad}  <hook NAME="FreeNode"/>\n` : '';
      const floats = d === 0 ? doc.floating.map((f) => rec(f, 1, 'right', true)).join('') : '';
      return `${pad}<node TEXT="${xmlEsc(n.text)}"${pos}${link}${folded}>\n${hook}${note}${n.children.map((c, i) => rec(c, d + 1, i % 2 ? 'left' : 'right')).join('')}${floats}${pad}</node>\n`;
    };
    return `<map version="1.0.1">\n${rec(doc.root, 0, 'right')}</map>\n`;
  }

  function fromFreeMind(text) {
    const x = parseXML(text);
    const map = deepFirst(x, 'map');
    const top = map ? first(map, 'node') : deepFirst(x, 'node');
    if (!top) throw new Error('Not a FreeMind file');
    const conv = (el) => {
      let t = el.attrs.TEXT;
      if (t == null) { const rc = kids(el, 'richcontent').find((r) => r.attrs.TYPE === 'NODE'); t = rc ? flatText(rc) : ''; }
      const n = M.createNode(t);
      if (el.attrs.LINK) n.link = el.attrs.LINK;
      if (el.attrs.FOLDED === 'true') n.collapsed = true;
      const note = kids(el, 'richcontent').find((r) => r.attrs.TYPE === 'NOTE');
      if (note) n.note = flatText(note).trim();
      if (el.attrs.POSITION === 'left') n.side = 'left';
      n.children = kids(el, 'node').map(conv);
      return n;
    };
    const isFree = (el) => kids(el, 'hook').some((h) => h.attrs.NAME === 'FreeNode');
    const freeEls = kids(top, 'node').filter(isFree);
    top.children = top.children.filter((c) => !freeEls.includes(c));
    const floating = freeEls.map(conv);
    floating.forEach((f) => { delete f.side; f._place = true; });
    return docFromTree(conv(top), floating);
  }
  function flatText(el) { return (el.text || '') + el.children.map((c) => flatText(c) + (/^(p|br|div|li)$/i.test(c.name) ? '\n' : '')).join(''); }

  // ---------- XMind (.xmind zip: content.json for XMind Zen/2020+, content.xml for XMind 8) ----------
  async function unzip(buffer) {
    const u8 = new Uint8Array(buffer);
    const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
    let eocd = -1;
    for (let i = u8.length - 22; i >= Math.max(0, u8.length - 65557); i--) {
      if (dv.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
    }
    if (eocd < 0) throw new Error('Not a zip archive');
    const count = dv.getUint16(eocd + 10, true);
    let p = dv.getUint32(eocd + 16, true);
    const files = {};
    for (let i = 0; i < count; i++) {
      if (dv.getUint32(p, true) !== 0x02014b50) break;
      const method = dv.getUint16(p + 10, true);
      const csize = dv.getUint32(p + 20, true);
      const nlen = dv.getUint16(p + 28, true), elen = dv.getUint16(p + 30, true), clen = dv.getUint16(p + 32, true);
      const off = dv.getUint32(p + 42, true);
      const name = new TextDecoder().decode(u8.subarray(p + 46, p + 46 + nlen));
      const lnlen = dv.getUint16(off + 26, true), lelen = dv.getUint16(off + 28, true);
      const start = off + 30 + lnlen + lelen;
      files[name] = { method, data: u8.subarray(start, start + csize) };
      p += 46 + nlen + elen + clen;
    }
    return {
      names: Object.keys(files),
      async read(name) {
        const f = files[name];
        if (!f) return null;
        if (f.method === 0) return new TextDecoder().decode(f.data);
        if (f.method !== 8) throw new Error('Unsupported zip compression');
        const ds = new DecompressionStream('deflate-raw');
        const stream = new Blob([f.data]).stream().pipeThrough(ds);
        return await new Response(stream).text();
      },
    };
  }

  async function fromXMind(buffer) {
    const z = await unzip(buffer);
    const json = await z.read('content.json');
    if (json) {
      const sheets = JSON.parse(json);
      const sheet = Array.isArray(sheets) ? sheets[0] : sheets;
      const conv = (t) => {
        const n = M.createNode(t.title || '');
        const notes = t.notes && (t.notes.plain || t.notes.realHTML);
        if (notes && notes.content) n.note = String(notes.content).replace(/<[^>]+>/g, '');
        if (t.labels && t.labels.length) n.tags = t.labels.slice();
        if (t.href && /^https?:/.test(t.href)) n.link = t.href;
        if (t.branch === 'folded') n.collapsed = true;
        const ch = (t.children && t.children.attached) || [];
        n.children = ch.map(conv);
        const det = (t.children && t.children.detached) || [];
        n._detached = det.map(conv);
        return n;
      };
      const root = conv(sheet.rootTopic);
      const floating = (root._detached || []).map((f) => Object.assign(f, { _place: true }));
      const strip = (n) => { delete n._detached; n.children.forEach(strip); };
      strip(root); floating.forEach(strip);
      const doc = docFromTree(root, floating);
      // relationships
      for (const r of sheet.relationships || []) { /* ids differ after conversion; skipped */ void r; }
      return doc;
    }
    const xml = await z.read('content.xml');
    if (!xml) throw new Error('This .xmind file has no content');
    const x = parseXML(xml);
    const sheet = deepFirst(x, 'sheet');
    const top = sheet ? first(sheet, 'topic') : deepFirst(x, 'topic');
    const conv = (el) => {
      const title = first(el, 'title');
      const n = M.createNode(title ? title.text : '');
      const notes = first(el, 'notes');
      if (notes) { const plain = first(notes, 'plain'); if (plain) n.note = plain.text.trim(); }
      if (el.attrs.branch === 'folded') n.collapsed = true;
      const ch = first(el, 'children');
      if (ch) {
        const att = kids(ch, 'topics').find((t) => t.attrs.type === 'attached') || first(ch, 'topics');
        if (att) n.children = kids(att, 'topic').map(conv);
      }
      return n;
    };
    return docFromTree(conv(top));
  }

  // ---------- detection ----------
  function importText(name, text) {
    const ext = (name.split('.').pop() || '').toLowerCase();
    const trimmed = text.trim();
    if (ext === 'mindloom' || ext === 'json' || trimmed.startsWith('{')) {
      const d = JSON.parse(text);
      return M.normalizeDoc(d.doc && d.doc.root ? d.doc : d);
    }
    if (ext === 'opml' || /<opml[\s>]/i.test(trimmed.slice(0, 300))) return fromOPML(text);
    if (ext === 'mm' || /^<map[\s>]/i.test(trimmed)) return fromFreeMind(text);
    if (ext === 'md' || ext === 'markdown' || /^#\s/m.test(trimmed)) return fromMarkdown(text);
    return fromText(text);
  }

  function serialize(doc) {
    const clean = JSON.parse(JSON.stringify(doc));
    return JSON.stringify(clean, null, 1);
  }

  ML.io = {
    parseXML, parseIndented, fromText, toText, toMarkdown, fromMarkdown, toOPML, fromOPML, toFreeMind, fromFreeMind,
    fromXMind, unzip, importText, serialize,
  };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
