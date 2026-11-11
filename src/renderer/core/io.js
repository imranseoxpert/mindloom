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
        const due = n.due ? ` (due: ${n.due})` : '';
        if (d === 0) out.push(`# ${text}${link}${due}`, '');
        else if (d === 1) out.push(`## ${task}${text}${link}${due}`, '');
        else out.push(`${'  '.repeat(d - 2)}- ${task}${text}${link}${due}`);
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
      const dm = /\(due:\s*(\d{4}-\d{2}-\d{2})\)/.exec(s);
      if (dm) s = s.replace(dm[0], '').trim();
      const n = M.createNode(clean(s));
      if (task) n.task = task;
      if (lm) n.link = lm[1];
      if (dm && M.isDateStr(dm[1])) n.due = dm[1];
      return n;
    }
    if (!roots.length) throw new Error('No headings or list items found');
    return docFromRoots(roots, (r) => floating.has(r));
  }

  // ---------- OPML ----------
  function toOPML(doc) {
    const rec = (n, d, fl) => {
      const pad = '  '.repeat(d + 2);
      const note = (n.note ? ` _note="${xmlEsc(n.note)}"` : '') + (n.due ? ` _due="${xmlEsc(n.due)}"` : '') + (fl ? ' _floating="true"' : '');
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
      if (el.attrs._due && M.isDateStr(el.attrs._due)) n.due = el.attrs._due;
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
      return `${pad}<node TEXT="${xmlEsc(n.text)}"${pos}${link}${folded}>\n${hook}${note}${n.children.map((c, i) => rec(c, d + 1, c.side || (i % 2 ? 'left' : 'right'))).join('')}${floats}${pad}</node>\n`;
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

  // ---------- CSV (spreadsheet export / import) ----------
  const csvCell = (v) => {
    const s = String(v == null ? '' : v);
    return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;
  };

  /** One row per topic, so a map opens cleanly in Excel / Google Sheets / Numbers. */
  function toCSV(doc) {
    const head = ['Topic', 'Path', 'Depth', 'Task', 'Due', 'Priority', 'Progress', 'Labels', 'Link', 'Note'];
    const rows = [head.join(',')];
    const emit = (tree, floating) => {
      const walkPath = (n, path, d) => {
        const here = path.concat([n.text.replace(/\n/g, ' ')]);
        const pri = (n.icons || []).find((m) => /^p\d$/.test(m));
        const prog = (n.icons || []).find((m) => /^pr\d+$/.test(m));
        rows.push([
          n.text.replace(/\n/g, ' '),
          (floating ? '(floating) ' : '') + here.join(' > '),
          d,
          n.task === 'done' ? 'Done' : n.task === 'todo' ? 'To do' : '',
          n.due || '',
          pri ? pri.slice(1) : '',
          prog ? prog.slice(2) + '%' : '',
          (n.tags || []).join('; '),
          n.link || '',
          (n.note || '').replace(/\n/g, ' '),
        ].map(csvCell).join(','));
        n.children.forEach((c) => walkPath(c, here, d + 1));
      };
      walkPath(tree, [], 0);
    };
    M.trees(doc).forEach((t, i) => emit(t, i > 0));
    return rows.join('\r\n') + '\r\n';
  }

  /** Minimal RFC 4180 parser: quoted cells, embedded commas / quotes / newlines, CRLF or LF. */
  function parseCSV(text) {
    const rows = [];
    let row = [], cell = '', quoted = false;
    const s = String(text).replace(/^\uFEFF/, '');
    for (let i = 0; i < s.length; i++) {
      const ch = s[i];
      if (quoted) {
        if (ch === '"') { if (s[i + 1] === '"') { cell += '"'; i++; } else quoted = false; }
        else cell += ch;
      } else if (ch === '"') quoted = true;
      else if (ch === ',') { row.push(cell); cell = ''; }
      else if (ch === '\n' || ch === '\r') {
        if (ch === '\r' && s[i + 1] === '\n') i++;
        row.push(cell); cell = '';
        if (row.length > 1 || row[0] !== '') rows.push(row);
        row = [];
      } else cell += ch;
    }
    row.push(cell);
    if (row.length > 1 || row[0] !== '') rows.push(row);
    return rows;
  }

  /**
   * Import a CSV. Mindloom's own export round-trips (Depth rebuilds the tree; Task / Due / Labels /
   * Link / Note are restored). A foreign CSV just needs a Topic / Title / Name column: rows become
   * topics under one root, nested by a Depth / Level column when there is one.
   */
  function fromCSV(text) {
    const rows = parseCSV(text);
    if (rows.length < 2) throw new Error('CSV has no data rows');
    const head = rows[0].map((h) => h.trim().toLowerCase());
    const col = (...names) => { for (const nm of names) { const i = head.indexOf(nm); if (i >= 0) return i; } return -1; };
    const ciTopic = col('topic', 'title', 'name', 'task name', 'subject');
    if (ciTopic < 0) throw new Error('CSV needs a Topic column');
    const ciDepth = col('depth', 'level', 'indent');
    const ciTask = col('task', 'status', 'state');
    const ciDue = col('due', 'due date', 'deadline', 'date');
    const ciTags = col('labels', 'tags', 'label', 'tag');
    const ciLink = col('link', 'url', 'hyperlink');
    const ciNote = col('note', 'notes', 'description', 'details');
    const get = (r, i) => (i >= 0 && r[i] != null ? String(r[i]).trim() : '');
    const nodes = [];
    for (const r of rows.slice(1)) {
      const topic = get(r, ciTopic);
      if (!topic) continue;
      const n = M.createNode(topic);
      const task = get(r, ciTask).toLowerCase();
      if (/done|complete|closed|yes|x/.test(task)) n.task = 'done';
      else if (task) n.task = 'todo';
      const due = get(r, ciDue);
      if (M.isDateStr(due)) n.due = due;
      const tags = get(r, ciTags);
      if (tags) n.tags = tags.split(/[;|]/).map((x) => x.trim()).filter(Boolean);
      const link = get(r, ciLink);
      if (/^https?:\/\//i.test(link)) n.link = link;
      const note = get(r, ciNote);
      if (note) n.note = note;
      let depth = parseInt(get(r, ciDepth), 10);
      if (!Number.isFinite(depth) || depth < 0) depth = nodes.length ? 1 : 0;
      nodes.push({ node: n, depth });
    }
    if (!nodes.length) throw new Error('CSV has no topics');
    // Rebuild from depths: the first depth-0 row is the root; a later depth-0 row starts a
    // floating tree (Mindloom's own CSV marks them in the Path column); deeper rows nest
    // under the last shallower one.
    let root = null;
    const floating = [];
    let stack = [];
    for (const { node, depth } of nodes) {
      if (depth === 0 || !stack.length) {
        if (!root) root = node;
        else { node._place = true; floating.push(node); }
        stack = [{ node, depth: 0 }];
        continue;
      }
      while (stack.length > 1 && stack[stack.length - 1].depth >= depth) stack.pop();
      stack[stack.length - 1].node.children.push(node);
      stack.push({ node, depth });
    }
    return docFromTree(root, floating);
  }

  // ---------- standalone HTML (viewable in any browser, no Mindloom needed) ----------
  function toHTML(doc, svg) {
    const escH = (s) => String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    const s = M.stats(doc);
    const list = (n) => {
      const badges = [
        n.task ? `<span class="badge">${n.task === 'done' ? '☑ done' : '☐ to do'}</span>` : '',
        n.due ? `<span class="badge${M.isOverdue(n) ? ' overdue' : ''}">📅 ${escH(M.formatDue(n.due))}${M.isOverdue(n) ? ' — overdue' : ''}</span>` : '',
        (n.tags || []).map((t) => `<span class="badge">${escH(t)}</span>`).join(''),
        n.link ? `<a href="${escH(n.link)}" rel="noopener">link</a>` : '',
      ].filter(Boolean).join(' ');
      const note = n.note ? `<div class="note">${escH(n.note).replace(/\n/g, '<br/>')}</div>` : '';
      const kids = n.children.length ? `<ul>${n.children.map(list).join('')}</ul>` : '';
      return `<li><span class="topic">${escH(n.text) || 'Topic'}</span>${badges ? ' ' + badges : ''}${note}${kids}</li>`;
    };
    const title = doc.root.text || 'Mind map';
    return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>${escH(title)} — Mindloom</title>
<style>body{font-family:-apple-system,'Segoe UI',Roboto,Arial,sans-serif;margin:0;padding:32px 20px 60px;background:#f6f7fb;color:#1d2440}main{max-width:980px;margin:0 auto}h1{font-size:26px;margin:0 0 6px}.meta{color:#5c677d;font-size:13px;margin-bottom:20px}.map{background:#fff;border:1px solid #dfe3ee;border-radius:12px;padding:12px;overflow:auto}.map svg{max-width:100%;height:auto}h2{font-size:18px;margin:28px 0 8px}ul{margin:6px 0 6px 18px;padding:0}li{margin:5px 0;line-height:1.55}.topic{font-weight:600}.badge{display:inline-block;background:#eef1f8;border-radius:999px;padding:1px 8px;font-size:11.5px;font-weight:600;color:#3c4666}.badge.overdue{background:#e03131;color:#fff}.note{color:#5c677d;font-size:13.5px;margin:3px 0 3px 2px}footer{color:#8a93ad;font-size:12px;margin-top:36px}</style>
</head><body><main>
<h1>${escH(title)}</h1>
<div class="meta">${s.nodes} topics · ${s.words} words${s.tasks ? ` · ${s.done}/${s.tasks} tasks done` : ''}${s.overdue ? ` · ${s.overdue} overdue` : ''}</div>
${svg ? `<div class="map">${svg}</div>` : ''}
<h2>Outline</h2>
<ul>${M.trees(doc).map(list).join('')}</ul>
<footer>Exported from Mindloom — a fast, friendly mind mapping app.</footer>
</main></body></html>
`;
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
    if (ext === 'csv') return fromCSV(text);
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
    fromXMind, unzip, importText, serialize, toCSV, fromCSV, parseCSV, toHTML,
  };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
