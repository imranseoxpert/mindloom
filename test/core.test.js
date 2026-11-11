'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

for (const f of ['themes', 'model', 'layout', 'render', 'io', 'templates']) require(`../src/renderer/core/${f}.js`);
const { model: M, layout: Lay, render: R, io: IO, themes: T, templates: TPL } = global.ML;

const measure = (text, font) => {
  const size = +(/(\d+)px/.exec(font) || [0, 14])[1];
  return text.length * size * 0.55;
};
const ctxFor = (doc, extra) => Object.assign({ theme: T.getTheme(doc.settings.theme), settings: doc.settings, measure }, extra || {});

function sampleDoc() {
  const doc = M.createDoc('Root');
  const a = M.addChild(doc, doc.root.id, 'A');
  const b = M.addChild(doc, doc.root.id, 'B');
  M.addChild(doc, a.id, 'A1');
  M.addChild(doc, a.id, 'A2');
  M.addChild(doc, b.id, 'B1');
  return { doc, a, b };
}

function overlaps(list) {
  const bad = [];
  for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length; j++) {
    const p = list[i], q = list[j];
    if (p.x < q.x + q.w - 0.5 && q.x < p.x + p.w - 0.5 && p.y < q.y + q.h - 0.5 && q.y < p.y + p.h - 0.5) bad.push([p.node.text, q.node.text]);
  }
  return bad;
}

test('model: add, sibling, move, reorder, remove, undo history', () => {
  const { doc, a, b } = sampleDoc();
  const s = M.addSibling(doc, a.id, 'A-sib');
  assert.equal(doc.root.children[1].id, s.id);
  assert.ok(M.move(doc, b.id, a.id));
  assert.equal(M.find(doc, b.id).parent.id, a.id);
  assert.equal(M.move(doc, a.id, b.id), false, 'cannot move into own descendant');
  assert.ok(M.reorder(doc, s.id, -1));
  assert.equal(doc.root.children[0].id, s.id);
  M.addRelation(doc, s.id, b.id, 'rel');
  assert.equal(doc.relations.length, 1);
  M.remove(doc, [a.id]);
  assert.equal(M.getNode(doc, b.id), null, 'subtree removed');
  assert.equal(doc.relations.length, 0, 'dangling relations cleaned');
  M.remove(doc, [doc.root.id]);
  assert.ok(doc.root, 'root cannot be removed');

  const h = new M.History();
  h.reset('1'); h.push('2'); h.push('3');
  assert.equal(h.undo(), '2');
  assert.equal(h.undo(), '1');
  assert.equal(h.undo(), null);
  assert.equal(h.redo(), '2');
  h.push('x');
  assert.equal(h.canRedo(), false);
});

test('model: indent/outdent, duplicate, detach to floating, search, markers', () => {
  const { doc, a, b } = sampleDoc();
  assert.ok(M.indent(doc, b.id));
  assert.equal(M.find(doc, b.id).parent.id, a.id);
  assert.ok(M.outdent(doc, b.id));
  assert.equal(M.find(doc, b.id).parent.id, doc.root.id);
  const d = M.duplicate(doc, a.id);
  assert.notEqual(d.id, a.id);
  assert.equal(d.children.length, 2);
  assert.notEqual(d.children[0].id, a.children[0].id);
  assert.ok(M.detach(doc, b.id, 300, 200));
  assert.equal(doc.floating.length, 1);
  assert.ok(M.find(doc, b.id).floating);
  assert.ok(M.move(doc, b.id, a.id), 'floating topic can be re-attached');
  assert.equal(doc.floating.length, 0);
  assert.deepEqual(M.search(doc, 'a1').length, 2);
  M.toggleMarker(a, 'p1');
  M.toggleMarker(a, 'p3');
  assert.deepEqual(a.icons, ['p3'], 'same marker group replaces');
  M.toggleMarker(a, 'p3');
  assert.equal(a.icons, undefined);
});

test('normalizeDoc repairs duplicate ids and missing fields', () => {
  const doc = M.normalizeDoc({ root: { id: 'x', text: 'R', children: [{ id: 'x', text: 'C' }, { text: 5 }] } });
  const ids = new Set();
  M.walkDoc(doc, (n) => ids.add(n.id));
  assert.equal(ids.size, 3);
  assert.equal(doc.root.children[1].text, '5');
  assert.equal(doc.settings.layout, 'mindmap');
  assert.throws(() => M.normalizeDoc({ nope: 1 }));
});

test('wrapText wraps long text and breaks long words', () => {
  const lines = Lay.wrapText('one two three four five six seven eight', '400 10px x', 60, measure);
  assert.ok(lines.length > 1);
  for (const l of lines) assert.ok(measure(l, '400 10px x') <= 60 || !l.includes(' '));
  const long = Lay.wrapText('a'.repeat(50), '400 10px x', 60, measure);
  assert.ok(long.length >= 4);
  assert.deepEqual(Lay.wrapText('a\n\nb', '400 10px x', 600, measure), ['a', '', 'b']);
});

test('every layout places all nodes without overlaps', () => {
  const doc = TPL.build('project');
  // add depth + an icon/tag/note to exercise measuring
  const deep = M.addChild(doc, doc.root.children[2].children[1].id, 'Deep child with a fairly long label that wraps');
  deep.tags = ['tag'];
  deep.note = 'note';
  deep.icons = ['p1', '🔥'];
  for (const lay of T.LAYOUTS.map((l) => l.id)) {
    doc.settings.layout = lay;
    const L = Lay.layout(doc, ctxFor(doc));
    let count = 0;
    M.walk(doc.root, () => { count++; });
    assert.equal(L.list.length, count, `${lay}: all nodes laid out`);
    for (const t of L.list) {
      assert.ok(Number.isFinite(t.x) && Number.isFinite(t.y) && t.w > 0 && t.h > 0, `${lay}: finite geometry`);
    }
    assert.deepEqual(overlaps(L.list), [], `${lay}: no overlapping nodes`);
    assert.equal(L.edges.length, count - 1, `${lay}: one connector per child`);
    for (const st of T.LINE_STYLES) for (const e of L.edges) {
      const p = Lay.edgePath(e, st.id);
      assert.ok(!/NaN|undefined/.test(p.d), `${lay}/${st.id}: valid path`);
    }
  }
});

test('collapsed nodes hide descendants; focus mode lays out a subtree', () => {
  const { doc, a } = sampleDoc();
  a.collapsed = true;
  let L = Lay.layout(doc, ctxFor(doc));
  assert.deepEqual(L.list.map((t) => t.node.text), ['Root', 'A', 'B', 'B1'], 'A1/A2 hidden');
  a.collapsed = false;
  L = Lay.layout(doc, ctxFor(doc, { focusId: a.id }));
  assert.deepEqual(L.list.map((t) => t.node.text), ['A', 'A1', 'A2']);
});

test('render produces well-formed SVG for every theme/shape/pattern', () => {
  const { doc, a, b } = sampleDoc();
  a.icons = ['p2', 'pr50', 'flag-red', 'star-blue', '💡'];
  a.task = 'done';
  a.link = 'https://example.com';
  a.note = 'hello <b>';
  a.tags = ['x&y'];
  a.boundary = true; a.boundaryLabel = 'Group';
  b.collapsed = true;
  M.addRelation(doc, a.id, b.id, 'depends on');
  M.detach(doc, a.children[1].id, 400, 300);
  for (const th of T.THEMES) for (const sh of T.SHAPES) {
    doc.settings.theme = th.id;
    doc.settings.shapes = { root: sh.id, main: sh.id, sub: sh.id };
    const L = Lay.layout(doc, ctxFor(doc));
    const svg = R.scene(L, doc, { theme: th, settings: doc.settings, selected: new Set([a.id]) });
    assert.ok(!/NaN|undefined/.test(svg), `${th.id}/${sh.id}`);
  }
  for (const p of T.PATTERNS) {
    doc.settings.pattern = p.id;
    const L = Lay.layout(doc, ctxFor(doc));
    const out = R.exportSVG(L, doc, { theme: T.getTheme(doc.settings.theme), settings: doc.settings });
    assert.match(out.svg, /^<svg[^>]+viewBox/);
    assert.ok(!out.svg.includes('<b>'), 'text is escaped');
    const opens = (out.svg.match(/<g[\s>]/g) || []).length, closes = (out.svg.match(/<\/g>/g) || []).length;
    assert.equal(opens, closes, 'balanced groups');
  }
});

test('markdown round-trip keeps structure, tasks and notes', () => {
  const { doc, a } = sampleDoc();
  a.children[0].task = 'done';
  a.note = 'A note';
  const md = IO.toMarkdown(doc);
  assert.match(md, /^# Root/);
  assert.match(md, /- \[x\] A1/);
  const back = IO.fromMarkdown(md);
  assert.equal(back.root.text, 'Root');
  assert.deepEqual(back.root.children.map((c) => c.text), ['A', 'B']);
  assert.equal(back.root.children[0].note, 'A note');
  assert.equal(back.root.children[0].children[0].task, 'done');
  assert.equal(back.root.children[0].children[1].text, 'A2');
});

test('OPML, FreeMind, indented text round-trips', () => {
  const { doc } = sampleDoc();
  doc.root.children[0].note = 'n & "q"';
  for (const [to, from] of [[IO.toOPML, IO.fromOPML], [IO.toFreeMind, IO.fromFreeMind], [IO.toText, IO.fromText]]) {
    const back = from(to(doc));
    assert.equal(back.root.text, 'Root');
    assert.deepEqual(back.root.children.map((c) => c.text), ['A', 'B']);
    assert.deepEqual(back.root.children[0].children.map((c) => c.text), ['A1', 'A2']);
  }
  assert.equal(IO.fromOPML(IO.toOPML(doc)).root.children[0].note, 'n & "q"');
  const multi = IO.fromText('- one\n  - two\n- three');
  assert.equal(multi.root.text, 'Imported');
  assert.equal(multi.root.children[0].children[0].text, 'two');
});

test('floating topics survive every text format round-trip', () => {
  const { doc, b } = sampleDoc();
  M.detach(doc, b.id, 300, 200);
  const extra = M.createNode('Lonely', { x: 10, y: 10 });
  doc.floating.push(extra);
  for (const [name, to, from] of [['md', IO.toMarkdown, IO.fromMarkdown], ['opml', IO.toOPML, IO.fromOPML], ['mm', IO.toFreeMind, IO.fromFreeMind], ['txt', IO.toText, IO.fromText]]) {
    const back = from(to(doc));
    assert.equal(back.root.text, 'Root', name);
    assert.deepEqual(back.root.children.map((c) => c.text), ['A'], name);
    assert.deepEqual(back.floating.map((f) => f.text), ['B', 'Lonely'], name);
    assert.equal(back.floating[0].children[0].text, 'B1', name);
    assert.ok(back.floating.every((f) => f._place), name + ': flagged for auto placement');
  }
  // a floating-only export never loses the main map
  assert.equal(IO.fromMarkdown('<!-- floating -->\n# Only').root.text, 'Only');
});

test('importText detects formats and native JSON round-trips', () => {
  const { doc } = sampleDoc();
  const json = IO.serialize(doc);
  const back = IO.importText('map.mindloom', json);
  assert.equal(back.root.children[0].children[1].text, 'A2');
  assert.equal(IO.importText('x.opml', IO.toOPML(doc)).root.text, 'Root');
  assert.equal(IO.importText('x.mm', IO.toFreeMind(doc)).root.text, 'Root');
  assert.equal(IO.importText('x.md', '# Hello\n- world').root.children[0].text, 'world');
});

test('XMind (.xmind zip with content.json) import', async () => {
  const content = JSON.stringify([{ rootTopic: { title: 'X Root', children: { attached: [{ title: 'Child', notes: { plain: { content: 'nn' } }, labels: ['L'], children: { attached: [{ title: 'Grand' }] } }], detached: [{ title: 'Float' }] } } }]);
  const zip = makeZip({ 'content.json': content, 'metadata.json': '{}' });
  const doc = await IO.fromXMind(zip);
  assert.equal(doc.root.text, 'X Root');
  assert.equal(doc.root.children[0].note, 'nn');
  assert.deepEqual(doc.root.children[0].tags, ['L']);
  assert.equal(doc.root.children[0].children[0].text, 'Grand');
  assert.equal(doc.floating[0].text, 'Float');
});

test('all templates build and lay out', () => {
  for (const t of TPL.TEMPLATES) {
    const doc = TPL.build(t.id);
    const L = Lay.layout(doc, ctxFor(doc));
    assert.ok(L.list.length >= 1, t.id);
    assert.deepEqual(overlaps(L.list), [], `${t.id} no overlaps`);
  }
});

test('example files in /examples are valid documents', () => {
  const dir = path.join(__dirname, '..', 'examples');
  if (!fs.existsSync(dir)) return;
  for (const f of fs.readdirSync(dir).filter((x) => x.endsWith('.mindloom'))) {
    const doc = IO.importText(f, fs.readFileSync(path.join(dir, f), 'utf8'));
    assert.ok(doc.root.text, f);
  }
});

// --- minimal zip writer (deflate) for the XMind test ---
function makeZip(files) {
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc32 = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const locals = [], centrals = [];
  let offset = 0;
  for (const [name, text] of Object.entries(files)) {
    const raw = Buffer.from(text), comp = zlib.deflateRawSync(raw), nameB = Buffer.from(name), crc = crc32(raw);
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0); lh.writeUInt16LE(20, 4); lh.writeUInt16LE(8, 8);
    lh.writeUInt32LE(crc, 14); lh.writeUInt32LE(comp.length, 18); lh.writeUInt32LE(raw.length, 22); lh.writeUInt16LE(nameB.length, 26);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0); ch.writeUInt16LE(20, 4); ch.writeUInt16LE(20, 6); ch.writeUInt16LE(8, 10);
    ch.writeUInt32LE(crc, 16); ch.writeUInt32LE(comp.length, 20); ch.writeUInt32LE(raw.length, 24); ch.writeUInt16LE(nameB.length, 28); ch.writeUInt32LE(offset, 42);
    locals.push(lh, nameB, comp); centrals.push(ch, nameB);
    offset += 30 + nameB.length + comp.length;
  }
  const cd = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(Object.keys(files).length, 8); end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(cd.length, 12); end.writeUInt32LE(offset, 16);
  const buf = Buffer.concat([...locals, cd, end]);
  return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
}

test('topic text never matches its own fill in any theme (main, sub, floating; rainbow on/off)', () => {
  for (const th of T.THEMES) for (const rainbow of [null, true, false]) {
    const settings = Object.assign({}, T.DEFAULT_SETTINGS, { theme: th.id, rainbow });
    for (const [depth, floating] of [[0, false], [1, false], [2, false], [0, true], [1, true]]) {
      for (let branch = 0; branch < th.branches.length; branch++) checkOne(th, settings, depth, floating, branch, rainbow);
    }
  }
  function checkOne(th, settings, depth, floating, branch, rainbow) {
    {
      const st = T.resolveStyle({ children: [] }, depth, branch, th, settings, floating);
      const bg = st.fill === 'transparent' ? th.bg : st.fill;
      assert.notEqual(st.color.toLowerCase(), bg.toLowerCase(), `${th.id} depth ${depth} floating ${floating} rainbow ${rainbow}`);
      const contrast = (Math.max(T.luminance(st.color), T.luminance(bg)) + 0.05) / (Math.min(T.luminance(st.color), T.luminance(bg)) + 0.05);
      assert.ok(contrast >= 2.2, `${th.id} depth ${depth} floating ${floating} rainbow ${rainbow}: contrast ${contrast.toFixed(2)} (${st.color} on ${bg})`);
    }
  }
});

test('manual offsets: a dragged topic stays connected, moves its branch, and keeps siblings in place', () => {
  for (const lay of ['mindmap', 'right', 'left', 'down', 'up', 'tree', 'radial']) {
    const doc = TPL.build('project');
    doc.settings.layout = lay;
    const before = Lay.layout(doc, ctxFor(doc));
    const target = doc.root.children[2];
    const pos = new Map(before.list.map((t) => [t.id, { x: t.x, y: t.y }]));
    target.offset = { x: 137, y: -91, l: lay };
    const after = Lay.layout(doc, ctxFor(doc));
    const inBranch = new Set();
    M.walk(target, (n) => inBranch.add(n.id));
    for (const t of after.list) {
      const p = pos.get(t.id);
      const dx = t.x - p.x, dy = t.y - p.y;
      if (inBranch.has(t.id)) assert.ok(Math.abs(dx - 137) < 0.01 && Math.abs(dy + 91) < 0.01, `${lay}: branch moved together`);
      else assert.ok(Math.abs(dx) < 0.01 && Math.abs(dy) < 0.01, `${lay}: ${t.node.text} did not move`);
    }
    assert.equal(after.edges.length, before.edges.length, `${lay}: still connected`);
    for (const st of T.LINE_STYLES) for (const e of after.edges) assert.ok(!/NaN|undefined/.test(Lay.edgePath(e, st.id).d), `${lay}/${st.id}`);
    // dragged across its parent: the connector re-routes instead of looping back
    target.offset = { x: lay === 'left' ? 900 : -900, y: 40, l: lay };
    const crossed = Lay.layout(doc, ctxFor(doc));
    const edge = crossed.edges.find((e) => e.to === target.id);
    assert.ok(Number.isFinite(edge.p1.x) && Number.isFinite(edge.p2.y), `${lay}: crossed edge valid`);
    // ignoreOffsetId reports the automatic slot
    const auto = Lay.layout(doc, ctxFor(doc, { ignoreOffsetId: target.id })).nodes.get(target.id);
    assert.ok(Math.abs(auto.x - pos.get(target.id).x) < 0.01, `${lay}: auto slot recoverable`);
    // offsets made in another structure are ignored
    target.offset = { x: 500, y: 500, l: 'other' };
    assert.ok(Math.abs(Lay.layout(doc, ctxFor(doc)).nodes.get(target.id).x - pos.get(target.id).x) < 0.01, `${lay}: foreign offset ignored`);
  }
  assert.equal(Lay.canFreePosition('fishbone'), false);
  assert.equal(Lay.canFreePosition('timeline', 1), false);
  assert.equal(Lay.canFreePosition('timeline', 2), true);
});

test('structural moves clear a manual offset; duplicates and pastes start clean', () => {
  const { doc, a, b } = sampleDoc();
  b.offset = { x: 10, y: 10, l: 'mindmap' };
  M.move(doc, b.id, a.id);
  assert.equal(b.offset, undefined);
  b.offset = { x: 10, y: 10, l: 'mindmap' };
  M.outdent(doc, b.id);
  assert.equal(b.offset, undefined);
  b.offset = { x: 10, y: 10, l: 'mindmap' };
  const pasted = M.insertSubtree(doc, a.id, b);
  assert.equal(pasted.offset, undefined);
});

test('normalizeDoc drops malformed or hostile values', () => {
  const doc = M.normalizeDoc({ root: { text: 'R', style: { fill: '"><img src=x onerror=alert(1)>', color: '#fff', fontSize: 'huge', shape: 'nope', width: 99999 }, image: { src: 'https://evil.example/x.png', w: 10, h: 10 },
    offset: { x: 'a', y: 1, l: 'mindmap' }, icons: 'p1', task: 'maybe', side: 'up', children: [null, { text: 'ok', style: { fill: '#ff0000', fontSize: 30 }, offset: { x: 5, y: 6, l: 'right' }, image: { src: 'data:image/png;base64,AAAA', w: 50, h: 40 } }] },
    relations: [], settings: { theme: 'zzz', layout: 'spiral', hGap: 1e9, lineWidth: 'x', shapes: { root: 'bad' } } });
  assert.deepEqual(doc.root.style, { color: '#fff', width: 2000 });
  assert.equal(doc.root.image, undefined);
  assert.equal(doc.root.offset, undefined);
  assert.equal(doc.root.icons, undefined);
  assert.equal(doc.root.task, undefined);
  assert.equal(doc.root.side, undefined);
  assert.equal(doc.root.children.length, 1);
  assert.deepEqual(doc.root.children[0].style, { fill: '#ff0000', fontSize: 30 });
  assert.deepEqual(doc.root.children[0].offset, { x: 5, y: 6, l: 'right' });
  assert.ok(doc.root.children[0].image);
  assert.equal(doc.settings.theme, 'fresh');
  assert.equal(doc.settings.layout, 'mindmap');
  assert.equal(doc.settings.hGap, 300);
  assert.equal(doc.settings.lineWidth, 2);
  assert.equal(doc.settings.shapes.root, 'rounded');
  const L = Lay.layout(doc, ctxFor(doc));
  assert.ok(!/NaN|undefined|onerror/.test(R.scene(L, doc, { theme: T.getTheme(doc.settings.theme), settings: doc.settings })));
});

test('history caps memory but always keeps the latest steps', () => {
  const h = new M.History(300, 100);
  h.reset('a'.repeat(60));
  h.push('b'.repeat(60));
  h.push('c'.repeat(60));
  assert.equal(h.stack.length, 2);
  assert.equal(h.undo()[0], 'b');
  assert.equal(h.undo(), null);
});

test('mind map sides are stable: adding or removing a main topic never moves the others across', () => {
  const doc = M.createDoc('Root');
  for (let i = 0; i < 6; i++) M.addChild(doc, doc.root.id, 'T' + i);
  const sides = () => new Map(Lay.layout(doc, ctxFor(doc)).root.children.map((t) => [t.node.text, t.side]));
  const first = sides();
  assert.deepEqual([...first.values()], ['right', 'right', 'right', 'left', 'left', 'left'], 'fresh map: clockwise fill');
  // the app persists sides after the first layout
  doc.root.children.forEach((n) => { n.side = first.get(n.text); });
  M.remove(doc, [doc.root.children[0].id]);
  M.addChild(doc, doc.root.id, 'New');
  const after = sides();
  for (const [k, v] of first) if (after.has(k)) assert.equal(after.get(k), v, k + ' kept its side');
  assert.equal(after.get('New'), 'right', 'new topic goes to the lighter side');
  assert.equal(IO.fromFreeMind(IO.toFreeMind(doc)).root.children.find((c) => c.text === 'T3').side, 'left');
});

test('due dates: validation, overdue, chip rendering, stats, markdown/OPML round-trip', () => {
  const { doc, a, b } = sampleDoc();
  assert.equal(M.isDateStr('2026-02-29'), false, '2026 is not a leap year');
  assert.equal(M.isDateStr('2024-02-29'), true);
  assert.equal(M.isDateStr('20 Nov 2026'), false);
  a.due = '2000-01-05'; // long past, not a task-done topic -> overdue
  b.due = '2999-12-31';
  b.task = 'done';
  assert.equal(M.isOverdue(a), true);
  assert.equal(M.isOverdue(b), false);
  assert.equal(M.formatDue('2000-01-05', new Date(2000, 5, 1)), '5 Jan');
  assert.equal(M.formatDue('2000-01-05', new Date(2001, 5, 1)), '5 Jan 2000');
  const st = M.stats(doc);
  assert.equal(st.due, 2);
  assert.equal(st.overdue, 1);
  // a hostile due value is dropped by normalizeDoc
  const bad = M.normalizeDoc({ root: { text: 'R', due: '2026-13-40', children: [{ text: 'C', due: '2026-11-20' }] } });
  assert.equal(bad.root.due, undefined);
  assert.equal(bad.root.children[0].due, '2026-11-20');
  // the chip is measured and drawn with the tags row
  const L = Lay.layout(doc, ctxFor(doc));
  const boxA = L.nodes.get(a.id).box;
  assert.ok(boxA.tags.some((t) => t.kind === 'due' && t.overdue), 'overdue due chip measured');
  const svg = R.scene(L, doc, { theme: T.getTheme(doc.settings.theme), settings: doc.settings });
  assert.match(svg, /class="tag due overdue"/);
  assert.ok(!/NaN|undefined/.test(svg));
  // round-trips
  assert.equal(IO.fromMarkdown(IO.toMarkdown(doc)).root.children[0].due, '2000-01-05');
  assert.equal(IO.fromOPML(IO.toOPML(doc)).root.children[0].due, '2000-01-05');
  // searching finds a topic by its due date, typed either way
  assert.ok(M.search(doc, '2000-01-05').includes(a.id));
  assert.ok(M.search(doc, '5 jan 2000').includes(a.id));
});

test('find & replace replaces in text and notes, case-insensitively, and reports the count', () => {
  const { doc, a } = sampleDoc();
  a.note = 'Alpha plan, alpha budget';
  a.children[0].text = 'ALPHA one';
  const n = M.replaceText(doc, 'alpha', 'beta');
  assert.equal(n, 3);
  assert.equal(a.note, 'beta plan, beta budget');
  assert.equal(a.children[0].text, 'beta one');
  assert.equal(M.replaceText(doc, 'alpha', 'x'), 0, 'nothing left to replace');
  assert.equal(M.replaceText(doc, '', 'x'), 0, 'empty search is a no-op');
  assert.equal(M.replaceText(doc, 'BETA', 'g', { caseSensitive: true }), 0, 'case-sensitive misses different case');
  const m = M.replaceText(doc, 'beta', 'g', { caseSensitive: true });
  assert.equal(m, 3);
  // the search text is literal, never a regex
  const { doc: d2 } = sampleDoc();
  d2.root.text = 'a.b aXb';
  assert.equal(M.replaceText(d2, 'a.b', 'z'), 1);
  assert.equal(d2.root.text, 'z aXb');
});

test('sortByDue orders dated subtopics first, earliest first', () => {
  const { doc, a } = sampleDoc();
  a.children[0].due = '2027-03-01';
  a.children[1].due = '2026-03-01';
  const extra = M.addChild(doc, a.id, 'Undated');
  assert.ok(M.sortByDue(doc, a.id));
  assert.deepEqual(a.children.map((c) => c.text), ['A2', 'A1', 'Undated']);
  assert.equal(M.sortByDue(doc, a.id), false, 'already sorted reports no change');
  assert.ok(extra);
});

test('CSV export/import round-trips structure, tasks, due dates, labels, links and notes', () => {
  const { doc, a, b } = sampleDoc();
  a.task = 'todo'; a.due = '2026-11-20'; a.tags = ['urgent', 'Q3']; a.link = 'https://example.com'; a.note = 'call, then "quote" it';
  b.task = 'done';
  const csv = IO.toCSV(doc);
  assert.match(csv, /^Topic,Path,Depth,Task,Due,Priority,Progress,Labels,Link,Note\r\n/);
  assert.match(csv, /"call, then ""quote"" it"/, 'CSV quoting');
  const back = IO.fromCSV(csv);
  assert.equal(back.root.text, 'Root');
  assert.deepEqual(back.root.children.map((c) => c.text), ['A', 'B']);
  assert.deepEqual(back.root.children[0].children.map((c) => c.text), ['A1', 'A2']);
  const ba = back.root.children[0];
  assert.equal(ba.task, 'todo');
  assert.equal(ba.due, '2026-11-20');
  assert.deepEqual(ba.tags, ['urgent', 'Q3']);
  assert.equal(ba.link, 'https://example.com');
  assert.equal(ba.note, 'call, then "quote" it');
  assert.equal(back.root.children[1].task, 'done');
  assert.equal(IO.importText('map.csv', csv).root.text, 'Root', 'importText detects .csv');
  // floating topics round-trip through CSV as their own trees
  const { doc: d3, b: b3 } = sampleDoc();
  M.detach(d3, b3.id, 300, 200);
  const back3 = IO.fromCSV(IO.toCSV(d3));
  assert.deepEqual(back3.root.children.map((c) => c.text), ['A']);
  assert.deepEqual(back3.floating.map((f) => f.text), ['B']);
  assert.equal(back3.floating[0].children[0].text, 'B1');
  // a foreign CSV with just a Topic column still imports
  const flat = IO.fromCSV('Topic,Owner\nPlan,Sam\nBuild,Jo\n');
  assert.equal(flat.root.text, 'Plan');
  assert.equal(flat.root.children[0].text, 'Build');
  assert.throws(() => IO.fromCSV('A,B\n1,2\n'), /Topic column/);
});

test('HTML export is a standalone page with the map SVG and an escaped outline', () => {
  const { doc, a } = sampleDoc();
  a.due = '2000-01-05';
  doc.root.text = 'Root <script>alert(1)</script>';
  const { svg } = (() => { const L = Lay.layout(doc, ctxFor(doc)); return R.exportSVG(L, doc, { theme: T.getTheme(doc.settings.theme), settings: doc.settings }); })();
  const html = IO.toHTML(doc, svg);
  assert.match(html, /^<!doctype html>/);
  assert.ok(html.includes('<svg'), 'embeds the map');
  assert.ok(html.includes('Root &lt;script&gt;'), 'title is escaped');
  assert.ok(!html.includes('<script>alert'), 'no injected script');
  assert.match(html, /overdue/, 'overdue badge shown');
  assert.match(html, /6 topics/, 'stats line present');
});

test('automatic numbering prefixes topics in layout only, never in the document', () => {
  const { doc, a, b } = sampleDoc();
  doc.settings.numbering = 'on';
  const L = Lay.layout(doc, ctxFor(doc));
  assert.equal(L.nodes.get(a.id).num, '1');
  assert.equal(L.nodes.get(b.id).num, '2');
  assert.equal(L.nodes.get(a.children[1].id).num, '1.2');
  assert.ok(L.nodes.get(a.id).box.lines[0].startsWith('1'), 'number is part of the measured first line');
  assert.equal(a.text, 'A', 'stored text is untouched');
  assert.deepEqual(overlaps(L.list), [], 'numbered topics do not overlap');
  const svg = R.scene(L, doc, { theme: T.getTheme(doc.settings.theme), settings: doc.settings });
  assert.ok(!/NaN|undefined/.test(svg));
  doc.settings.numbering = 'off';
  const L2 = Lay.layout(doc, ctxFor(doc));
  assert.equal(L2.nodes.get(a.id).num, '');
  // an unknown numbering value in a file falls back to off
  assert.equal(M.normalizeDoc({ root: { text: 'R' }, settings: { numbering: 'roman' } }).settings.numbering, 'off');
});

test('AI: base URL safety, model filtering and answer cleaning', () => {
  const AI = require('../src/main/ai.js');
  assert.equal(AI.validateBaseUrl('https://api.example.com/v1/'), 'https://api.example.com/v1');
  assert.equal(AI.validateBaseUrl('http://localhost:11434/v1'), 'http://localhost:11434/v1');
  assert.equal(AI.validateBaseUrl('http://192.168.1.20:1234/v1'), 'http://192.168.1.20:1234/v1');
  assert.throws(() => AI.validateBaseUrl('http://api.example.com/v1'), /https/);
  assert.throws(() => AI.validateBaseUrl('https://user:pw@api.example.com'), /key field/);
  assert.throws(() => AI.validateBaseUrl('file:///etc/passwd'), /https/);
  assert.throws(() => AI.validateBaseUrl('not a url'), /full address/);
  assert.equal(AI.isPrivateHost('172.20.0.5'), true);
  assert.equal(AI.isPrivateHost('172.40.0.5'), false);
  const models = AI.chatModels([{ id: 'old-chat', created: 10 }, { id: 'text-embedding-3', created: 99 }, { id: 'new-chat', created: 20 }, { id: 'whisper-1' }, { id: 'new-chat', created: 20 }, { id: 'models/gemini-x', created_at: '2026-01-01' }]);
  assert.deepEqual(models.map((m) => m.id), ['gemini-x', 'new-chat', 'old-chat']);
  assert.equal(AI.chatText({ choices: [{ message: { content: [{ type: 'text', text: 'a' }, { type: 'text', text: 'b' }] } }] }), 'ab');
  assert.equal(AI.cleanOutline('<think>hmm\nlong</think>\nSure! Here is your map:\n```text\n# Root\n  - **One**\n\n  2. Two\n```'), 'Root\n  One\n  Two');
  assert.equal(AI.cleanOutline('Sure-fire ideas\n  Quick wins'), 'Sure-fire ideas\n  Quick wins', 'real topics are never dropped');
  // every provider has a name and either a fixed address, an editable one, or the SDK
  for (const p of AI.PROVIDERS) assert.ok(p.name && (p.kind === 'anthropic' || p.base || p.editableBase), p.id);
  assert.ok(AI.PROVIDERS.length >= 10);
  assert.match(AI.buildPrompt({ mode: 'text', text: 'abc' }), /<text>\nabc\n<\/text>/);
  assert.match(AI.buildPrompt({ mode: 'expand', path: ['A', 'B'], existing: ['x'] }), /A > B/);
});
