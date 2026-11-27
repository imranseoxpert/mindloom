'use strict';
/*
 * Records what the JavaScript engine (src/renderer/core) does, so the Rust engine can be held to it (ADR-005, ADR-010).
 * Run: node rust/record.js   → rust/cases/*.json.gz and rust/core/src/data.rs (themes + templates, generated)
 */
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

for (const f of ['themes', 'model', 'layout', 'render', 'io', 'templates']) require(`../src/renderer/core/${f}.js`);
const { model: M, layout: Lay, render: R, io: IO, themes: T, templates: TPL } = global.ML;
const OUT = path.join(__dirname, 'cases');
fs.mkdirSync(OUT, { recursive: true });

// fixed text measure (ADR-005): UTF-16 length × px size × 0.55
const measure = (text, font) => text.length * +(/(\d+)px/.exec(font) || [0, 14])[1] * 0.55;

// deterministic randomness
let seed = 1;
const rnd = () => { seed = (seed * 1103515245 + 12345) % 2147483648; return seed / 2147483648; };
const pick = (a) => a[Math.floor(rnd() * a.length)];
const chance = (p) => rnd() < p;
const int = (a, b) => a + Math.floor(rnd() * (b - a + 1));

const TEXTS = ['Idea', 'Plan the launch', 'Research competitors and pricing', 'A', 'Supercalifragilisticexpialidocious-extra-long-word-here',
  'Two\nlines', 'Line one\n\nafter a blank', '  padded  ', 'Café crème', 'naïve façade', '日本語のテキスト', '😀 emoji start', 'mixed 🚀 rocket text',
  'é combining', 'tab\tinside', 'nbsp space', 'ends with space ', 'Q3', 'Budget: $1,200 & <notes> "quoted"', 'Ünïcödé Ümlauts',
  'Very long topic text that keeps going and going so it has to wrap across several lines of the box', '#hashtag', 'x', ''];
const COLORS = ['#ffd43b', '#1c7ed6', '#ffffff', '#000000', '#e03131', 'transparent', '#2f9e44', '#abc'];
let idn = 0;
function genNode(depth, ctx) {
  const n = { id: 'n' + idn++, text: pick(TEXTS), children: [] };
  if (chance(0.25)) {
    const st = {};
    if (chance(0.4)) st.fill = pick(COLORS);
    if (chance(0.3)) st.color = pick(COLORS.filter((c) => c !== 'transparent'));
    if (chance(0.3)) st.border = pick(COLORS);
    if (chance(0.4)) st.shape = pick(T.SHAPES).id;
    if (chance(0.3)) st.fontSize = pick([12, 16, 20, 28]);
    if (chance(0.3)) st.bold = chance(0.5);
    if (chance(0.2)) st.italic = true;
    if (chance(0.2)) st.underline = true;
    if (chance(0.2)) st.strike = true;
    if (chance(0.3)) st.align = pick(['left', 'center', 'right']);
    if (chance(0.2)) st.font = pick(T.FONTS).id;
    if (chance(0.2)) st.width = pick([120, 200, 400]);
    if (chance(0.2)) st.borderWidth = pick([0, 2, 3]);
    if (chance(0.2)) st.borderDash = pick(['dashed', 'dotted']);
    if (chance(0.2)) st.lineColor = pick(COLORS);
    if (Object.keys(st).length) n.style = st;
  }
  if (chance(0.2)) n.icons = [...new Set([pick(T.MARKER_GROUPS).items[0], pick(pick(T.MARKER_GROUPS).items)])];
  if (chance(0.15)) n.task = pick(['todo', 'done']);
  if (chance(0.15)) n.tags = [pick(['urgent', 'Q3', 'idée', 'a&b'])].concat(chance(0.4) ? ['two words'] : []);
  if (chance(0.12)) n.note = pick(['A note', 'Line 1\nLine 2', 'x'.repeat(320), '<b>bold</b> & "q"']);
  if (chance(0.1)) n.link = 'https://example.com/?a=1&b=2';
  if (chance(0.06)) n.image = { src: 'data:image/png;base64,AAAA', w: pick([80, 240, 600]), h: pick([60, 160]) };
  if (depth === 1 && chance(0.3)) n.side = pick(['left', 'right']);
  if (depth >= 1 && chance(0.12)) n.offset = { x: int(-200, 200), y: int(-150, 150), l: chance(0.8) ? ctx.layout : pick(['mindmap', 'right', 'float', 'down']) };
  if (chance(0.06)) { n.boundary = true; if (chance(0.5)) n.boundaryLabel = pick(['Group', 'Make it yours', 'Ünï']); }
  const kids = depth === 0 ? int(0, 7) : depth < 4 ? (chance(0.55) ? int(1, 4) : 0) : 0;
  for (let i = 0; i < kids && idn < ctx.limit; i++) n.children.push(genNode(depth + 1, ctx));
  if (n.children.length && depth > 0 && chance(0.1)) n.collapsed = true;
  return n;
}
function genSettings() {
  return {
    theme: pick(T.THEMES).id, layout: pick(T.LAYOUTS).id, lineStyle: pick(T.LINE_STYLES).id, pattern: pick(T.PATTERNS).id, font: pick(T.FONTS).id,
    shapes: { root: pick(T.SHAPES).id, main: pick(T.SHAPES).id, sub: pick(T.SHAPES).id },
    rainbow: pick([null, true, false]), hGap: pick([30, 56, 90]), vGap: pick([6, 14, 24]), lineWidth: pick([1, 2, 3.5]), maxWidth: pick([160, 260]), fontScale: pick([0.8, 1, 1.25]),
  };
}
function genDoc() {
  idn = 0;
  const settings = genSettings();
  const ctx = { layout: settings.layout, limit: int(1, 45) };
  const root = genNode(0, ctx);
  const floating = [];
  for (let i = 0, k = chance(0.3) ? int(1, 2) : 0; i < k; i++) { const f = genNode(1, ctx); delete f.side; f.x = int(-400, 400); f.y = int(-300, 300); floating.push(f); }
  const ids = [];
  M.walk(root, (x) => { ids.push(x.id); });
  floating.forEach((f) => M.walk(f, (x) => { ids.push(x.id); }));
  const relations = [];
  for (let i = 0, k = ids.length > 2 && chance(0.35) ? int(1, 3) : 0; i < k; i++) {
    const r = { id: 'r' + i, from: pick(ids), to: pick(ids), label: pick(['', 'depends on', 'Ünï & <x>']) };
    if (chance(0.3)) r.color = pick(['#e03131', '#1c7ed6']);
    if (chance(0.3)) r.bend = int(-150, 150);
    relations.push(r);
  }
  return { app: 'mindloom', version: 1, root, floating, relations, settings };
}

// ---------- helpers to describe results ----------
const pt = (p) => (p ? [p.x, p.y] : null);
function layoutSummary(L) {
  return {
    list: L.list.map((t) => ({
      id: t.id, x: t.x, y: t.y, w: t.w, h: t.h, depth: t.depth, branch: t.branch, dir: t.dir, side: t.side || null, moved: !!t.moved,
      lines: t.box.lines, lh: t.box.lh, row: t.box.row, tagsX: t.box.tagsX, tagsY: t.box.tagsY, tags: t.box.tags, img: t.box.img, imgPos: t.box.imgPos,
      style: t.box.style,
    })),
    edges: L.edges.map((e) => ({ from: e.from, to: e.to, orient: e.orient, p1: pt(e.p1), p2: pt(e.p2), color: e.color, width: e.width, depth: e.depth,
      paths: Object.fromEntries(T.LINE_STYLES.map((s) => { const p = Lay.edgePath(e, s.id); return [s.id, p.filled ? 'F' + p.d : p.d]; })) })),
    boundaries: L.boundaries,
    extras: L.extras,
    bounds: L.bounds,
  };
}
/** Doc with ids replaced by their walk order, so structure can be compared without random ids. */
function canon(doc) {
  const map = new Map();
  let k = 0;
  const visit = (n) => { map.set(n.id, 'i' + k++); n.children.forEach(visit); };
  visit(doc.root); doc.floating.forEach(visit);
  const fix = (n) => { const c = Object.assign({}, n); c.id = map.get(n.id); c.children = n.children.map(fix); return c; };
  return { root: fix(doc.root), floating: doc.floating.map(fix), relations: doc.relations.map((r) => Object.assign({}, r, { id: 'r', from: map.get(r.from), to: map.get(r.to) })), settings: doc.settings };
}
function write(name, cases) {
  fs.writeFileSync(path.join(OUT, name + '.json.gz'), zlib.gzipSync(JSON.stringify(cases)));
  console.log(`${name}: ${cases.length} cases`);
}

// ---------- 1. layout + render ----------
const layoutCases = [];
const docs = [];
for (const t of TPL.TEMPLATES) for (const lay of T.LAYOUTS) { const d = TPL.build(t.id); d.settings.layout = lay.id; docs.push(JSON.parse(JSON.stringify(d))); }
for (let i = 0; i < 260; i++) docs.push(genDoc());
for (const raw of docs) {
  const doc = M.normalizeDoc(JSON.parse(JSON.stringify(raw)));
  const input = JSON.parse(JSON.stringify(doc));
  const theme = T.getTheme(doc.settings.theme);
  const ctx = { theme, settings: doc.settings, measure };
  const L = Lay.layout(doc, ctx);
  const ids = L.list.map((t) => t.id);
  const sel = ids.filter((_, i) => i % 3 === 1).slice(0, 3);
  const rel = doc.relations[0] ? doc.relations[0].id : null;
  const c = {
    doc: input,
    layout: layoutSummary(L),
    scene: R.scene(L, doc, { theme, settings: doc.settings, selected: new Set(sel), matches: new Set(ids.slice(0, 1)), selectedRel: rel }),
    selected: sel, matches: ids.slice(0, 1), selectedRel: rel,
    svg: R.exportSVG(L, doc, { theme, settings: doc.settings }).svg,
    svgTransparent: R.exportSVG(L, doc, { theme, settings: doc.settings, transparent: true, padding: 16 }).svg,
  };
  const focus = doc.root.children.find((x) => x.children.length);
  if (focus) c.focus = { id: focus.id, layout: layoutSummary(Lay.layout(doc, Object.assign({}, ctx, { focusId: focus.id }))) };
  let off = null;
  M.walkDoc(doc, (n) => { if (!off && n.offset) off = n; });
  if (off) c.ignore = { id: off.id, layout: layoutSummary(Lay.layout(doc, Object.assign({}, ctx, { ignoreOffsetId: off.id }))) };
  layoutCases.push(c);
}
write('layout', layoutCases);

// ---------- 2. text wrapping ----------
const wrapCases = [];
for (const text of TEXTS.concat(['a b c d e f g h i j k l m n o p', 'word  double  spaces', '　ideographic　space', 'trailing\n', '\n', 'x y', 'zero​width', '﻿bom start'])) {
  for (const maxW of [20, 60, 120, 400]) for (const size of [10, 14, 24]) wrapCases.push({ text, maxW, size, out: Lay.wrapText(text, `400 ${size}px x`, maxW, measure) });
}
write('wrap', wrapCases);

// ---------- 3. styles ----------
const styleCases = [];
const nodeStyles = [{}, { fill: '#ffd43b' }, { fill: 'transparent' }, { fill: '#000000', color: '#ff0000' }, { shape: 'hexagon', fontSize: 20, bold: false, italic: true }, { borderWidth: 0, lineColor: '#123456', font: 'serif', align: 'left' }];
for (const th of T.THEMES) for (const rainbow of [null, true, false]) for (const fsc of [1, 1.25]) for (const [depth, fl] of [[0, false], [1, false], [2, false], [3, false], [0, true], [1, true]]) for (const branch of [-1, 0, 3, 9]) for (const ns of nodeStyles) {
  const settings = Object.assign({}, T.DEFAULT_SETTINGS, { theme: th.id, rainbow, fontScale: fsc, shapes: { root: 'ellipse', main: 'pill', sub: 'underline' } });
  styleCases.push({ theme: th.id, rainbow, fontScale: fsc, depth, floating: fl, branch, style: ns, out: T.resolveStyle({ style: Object.keys(ns).length ? ns : undefined, children: [] }, depth, branch, th, settings, fl) });
}
write('style', styleCases);

// ---------- 4. text formats ----------
const ioCases = [];
for (const raw of docs.slice(0, 200)) {
  const doc = M.normalizeDoc(JSON.parse(JSON.stringify(raw)));
  const c = { doc: JSON.parse(JSON.stringify(doc)), md: IO.toMarkdown(doc), opml: IO.toOPML(doc), txt: IO.toText(doc), mm: IO.toFreeMind(doc) };
  c.back = {};
  for (const [k, fn] of [['md', IO.fromMarkdown], ['opml', IO.fromOPML], ['txt', IO.fromText], ['mm', IO.fromFreeMind]]) {
    try { c.back[k] = canon(fn(c[k])); } catch (e) { c.back[k] = { error: e.message }; }
  }
  ioCases.push(c);
}
const HAND = [
  ['x.md', '# Title\n\nSome intro text\n\n## Section\n- item one\n  - nested **bold** [link](https://a.b)\n- [x] done task\n- [ ] open task\n> a quote note\n\n```\ncode is skipped\n```\n### Deeper\n1. numbered\n2) also'],
  ['x.md', '- a\n- b\n  - c'], ['x.md', 'no structure here'], ['x.md', '<!-- floating -->\n# Only'],
  ['x.txt', 'Root\n\tChild\n\t\tGrand\n\tChild 2\n\nFloat\n  Fchild'], ['x.txt', '   '], ['x.txt', 'One\nTwo\nThree'],
  ['x.opml', '<?xml version="1.0"?><opml><head><title>T &amp; U</title></head><body><outline text="A" _note="n&#10;2"><outline text="B"/></outline><outline title="C"/></body></opml>'],
  ['x.opml', '<opml><body></body></opml>'], ['x.opml', '<notopml/>'],
  ['x.mm', '<map version="1.0.1"><node TEXT="Root"><node TEXT="L" POSITION="left" FOLDED="true" LINK="https://x.y"><node TEXT="g"/></node><node><richcontent TYPE="NODE"><html><body><p>Rich</p></body></html></richcontent><richcontent TYPE="NOTE"><html><body><p>Note text</p></body></html></richcontent></node><node TEXT="free"><hook NAME="FreeNode"/></node></node></map>'],
  ['x.mindloom', JSON.stringify({ root: { text: 'R', children: [{ text: 'C' }] } })], ['x.json', '{"doc":{"root":{"text":"Wrapped","children":[]}}}'], ['x.json', '{"nope":1}'],
  ['x.mm', '<map><nothing/></map>'], ['x.md', '# A\n# B\n## B1'],
];
const handCases = HAND.map(([name, text]) => { try { return { name, text, out: canon(IO.importText(name, text)) }; } catch (e) { return { name, text, error: e.message }; } });
write('io', ioCases);
write('import', handCases);

// ---------- 5. model operations ----------
const opCases = [];
const pathOf = (doc, id) => { const p = []; const rec = (n, acc) => { if (n.id === id) { p.push(...acc); return true; } return n.children.some((c, i) => rec(c, acc.concat(i))); }; if (rec(doc.root, [])) return ['r'].concat(p); for (let k = 0; k < doc.floating.length; k++) { p.length = 0; if (rec(doc.floating[k], [])) return ['f' + k].concat(p); } return null; };
const OPS = ['addChild', 'addSibling', 'addSiblingBefore', 'addParent', 'remove', 'move', 'detach', 'reorderUp', 'reorderDown', 'indent', 'outdent', 'duplicate', 'insertSubtree', 'collapse', 'collapseToLevel', 'marker', 'relation', 'search', 'stats'];
for (let i = 0; i < 400; i++) {
  const doc = M.normalizeDoc(genDoc());
  const ids = []; M.walkDoc(doc, (n) => ids.push(n.id));
  const op = OPS[i % OPS.length];
  const a = pick(ids), b = pick(ids);
  const before = canon(JSON.parse(JSON.stringify(doc)));
  const args = { a: pathOf(doc, a), b: pathOf(doc, b), n: int(0, 3), text: pick(TEXTS), marker: pick(pick(T.MARKER_GROUPS).items), q: pick(['idea', 'É', 'plan', 'zz', '']), x: int(-50, 50), y: int(-50, 50), index: pick([null, 0, 1, 99]), flag: pick([null, true, false]) };
  let result;
  try {
    switch (op) {
      case 'addChild': result = !!M.addChild(doc, a, args.text, args.index); break;
      case 'addSibling': result = !!M.addSibling(doc, a, args.text, false); break;
      case 'addSiblingBefore': result = !!M.addSibling(doc, a, args.text, true); break;
      case 'addParent': result = !!M.addParent(doc, a, args.text); break;
      case 'remove': { const nx = M.remove(doc, [a, b]); result = nx ? pathOf(doc, nx) : null; break; }
      case 'move': result = M.move(doc, a, b, args.index); break;
      case 'detach': result = M.detach(doc, a, args.x, args.y); break;
      case 'reorderUp': result = M.reorder(doc, a, -1); break;
      case 'reorderDown': result = M.reorder(doc, a, 1); break;
      case 'indent': result = M.indent(doc, a); break;
      case 'outdent': result = M.outdent(doc, a); break;
      case 'duplicate': result = !!M.duplicate(doc, a); break;
      case 'insertSubtree': result = !!M.insertSubtree(doc, b, JSON.parse(JSON.stringify(M.getNode(doc, a))), args.index); break;
      case 'collapse': result = M.setCollapsed(doc, a, args.flag); break;
      case 'collapseToLevel': M.collapseToLevel(doc, args.n); result = null; break;
      case 'marker': M.toggleMarker(M.getNode(doc, a), args.marker); result = null; break;
      case 'relation': result = !!M.addRelation(doc, a, b, args.text); break;
      case 'search': result = M.search(doc, args.q).map((id) => pathOf(doc, id)); break;
      case 'stats': result = M.stats(doc); break;
      default: throw new Error(op);
    }
  } catch (e) { result = { error: e.message }; }
  opCases.push({ op, args, doc: before, result, after: canon(doc) });
}
const NORMALIZE = [
  { root: { id: 'x', text: 'R', children: [{ id: 'x', text: 'C' }, { text: 5 }] } },
  { root: { text: 'R', style: { fill: '"><img src=x>', color: '#fff', fontSize: 'huge', shape: 'nope', width: 99999 }, image: { src: 'https://evil/x.png', w: 10, h: 10 }, offset: { x: 'a', y: 1, l: 'mindmap' }, icons: 'p1', task: 'maybe', side: 'up', children: [null, { text: 'ok', style: { fill: '#ff0000', fontSize: 30 }, offset: { x: 5, y: 6, l: 'right' }, image: { src: 'data:image/png;base64,AAAA', w: 50, h: 40 } }] }, relations: [{ from: 'q', to: 'w' }], settings: { theme: 'zzz', layout: 'spiral', hGap: 1e9, lineWidth: 'x', shapes: { root: 'bad' } } },
  { root: { text: 'R', children: [] }, floating: [{ id: 'f', text: 'F', children: [] }, { text: 'G', x: '12', y: null, children: [] }], relations: [{ id: 'r1', from: 'f', to: 'f', label: 7, color: 'red', bend: 'x' }] },
  { nope: 1 }, null,
];
const normCases = NORMALIZE.map((d) => { try { return { doc: d, out: canon(M.normalizeDoc(JSON.parse(JSON.stringify(d)))) }; } catch (e) { return { doc: d, error: e.message }; } });
write('ops', opCases);
write('normalize', normCases);

// ---------- 6. generated Rust data: themes and built templates, straight from the JS ----------
const rs = (v) => JSON.stringify(v);
const lines = ['// @generated by rust/record.js from src/renderer/core/themes.js and templates.js — do not edit.', 'use crate::themes::*;', ''];
lines.push('pub static THEMES: &[Theme] = &[');
for (const t of T.THEMES) {
  const lvl = (l) => `Level { fill: ${rs(l.fill)}, color: ${rs(l.color)}, border: ${rs(l.border)} }`;
  lines.push(`    Theme { id: ${rs(t.id)}, name: ${rs(t.name)}, dark: ${t.dark}, bg: ${rs(t.bg)}, ink: ${rs(t.ink)}, root: ${lvl(t.root)}, main: ${lvl(t.main)}, sub: ${lvl(t.sub)}, line: ${rs(t.line)}, rainbow: ${t.rainbow}, branches: &[${t.branches.map(rs).join(', ')}] },`);
}
lines.push('];', '');
const named = (name, arr) => { lines.push(`pub static ${name}: &[Named] = &[${arr.map((x) => `Named { id: ${rs(x.id)}, name: ${rs(x.name)} }`).join(', ')}];`); };
named('LAYOUTS', T.LAYOUTS); named('LINE_STYLES', T.LINE_STYLES); named('SHAPES', T.SHAPES); named('PATTERNS', T.PATTERNS);
lines.push(`pub static FONTS: &[Font] = &[${T.FONTS.map((x) => `Font { id: ${rs(x.id)}, name: ${rs(x.name)}, css: ${rs(x.css)} }`).join(', ')}];`);
lines.push(`pub static MAP_STYLES: &[MapStyle] = &[\n    ${T.MAP_STYLES.map((m) => `MapStyle { id: ${rs(m.id)}, name: ${rs(m.name)}, layout: ${rs(m.layout)}, line_style: ${rs(m.lineStyle)}, shapes: [${rs(m.shapes.root)}, ${rs(m.shapes.main)}, ${rs(m.shapes.sub)}], font: ${m.font ? `Some(${rs(m.font)})` : 'None'} }`).join(',\n    ')},\n];`);
lines.push(`pub static MARKER_GROUPS: &[MarkerGroup] = &[\n    ${T.MARKER_GROUPS.map((g) => `MarkerGroup { name: ${rs(g.name)}, items: &[${g.items.map(rs).join(', ')}] }`).join(',\n    ')},\n];`);
lines.push(`pub static PRIORITY_COLORS: &[&str] = &[${T.PRIORITY_COLORS.map(rs).join(', ')}];`);
lines.push(`pub static FLAG_COLORS: &[(&str, &str)] = &[${Object.entries(T.FLAG_COLORS).map(([k, v]) => `(${rs(k)}, ${rs(v)})`).join(', ')}];`);
lines.push(`pub static STAR_COLORS: &[(&str, &str)] = &[${Object.entries(T.STAR_COLORS).map(([k, v]) => `(${rs(k)}, ${rs(v)})`).join(', ')}];`);
lines.push(`pub static SWATCHES: &[&str] = &[${T.SWATCHES.map(rs).join(', ')}];`, '');
lines.push('/// Each template as the JS builds it (outline, settings and decorations), ids replaced by walk order.');
lines.push('pub static TEMPLATES: &[Template] = &[');
for (const t of TPL.TEMPLATES) lines.push(`    Template { id: ${rs(t.id)}, name: ${rs(t.name)}, icon: ${rs(t.icon)}, desc: ${rs(t.desc)}, doc: ${rs(JSON.stringify(canon(TPL.build(t.id))))} },`);
lines.push('];');
fs.mkdirSync(path.join(__dirname, 'core', 'src'), { recursive: true });
fs.writeFileSync(path.join(__dirname, 'core', 'src', 'data.rs'), lines.join('\n') + '\n');
console.log('core/src/data.rs written');
