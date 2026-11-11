'use strict';
/*
 * Mindloom end-to-end self test. Runs inside the real Electron app (dev or packaged) when
 * MINDLOOM_SMOKE=<output dir> is set. Drives the UI with real mouse/keyboard input events,
 * exercises every layout/theme, export, save and reload, then exits 0 (pass) or 1 (fail).
 */
const fs = require('node:fs');
const path = require('node:path');
const { app } = require('electron');

exports.run = async function run(win, { svgToPdf }) {
  const out = process.env.MINDLOOM_SMOKE;
  fs.mkdirSync(out, { recursive: true });
  const wc = win.webContents;
  const results = [];
  const errors = [];
  wc.on('console-message', (e) => { if (e.level === 'error' || e.level === 3) errors.push(e.message); });
  wc.on('render-process-gone', (e, d) => errors.push('renderer gone: ' + d.reason));
  wc.on('preload-error', (e, p, err) => errors.push('preload error: ' + err.message));

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const js = (code) => wc.executeJavaScript(code, true);
  const check = (name, ok, detail) => { results.push({ name, ok: !!ok, detail: detail == null ? '' : String(detail) }); if (!ok) console.log(`  FAIL ${name} ${detail == null ? '' : detail}`); };
  const key = async (keyCode, modifiers) => {
    wc.sendInputEvent({ type: 'keyDown', keyCode, modifiers: modifiers || [] });
    wc.sendInputEvent({ type: 'keyUp', keyCode, modifiers: modifiers || [] });
    await sleep(60);
  };
  const type = async (text) => {
    for (const ch of text) {
      wc.sendInputEvent({ type: 'keyDown', keyCode: ch });
      wc.sendInputEvent({ type: 'char', keyCode: ch });
      wc.sendInputEvent({ type: 'keyUp', keyCode: ch });
    }
    await sleep(80);
  };
  const click = async (p, count) => {
    for (let c = 1; c <= (count || 1); c++) {
      wc.sendInputEvent({ type: 'mouseDown', x: p.x, y: p.y, button: 'left', clickCount: c });
      wc.sendInputEvent({ type: 'mouseUp', x: p.x, y: p.y, button: 'left', clickCount: c });
    }
    await sleep(120);
  };
  const drag = async (a, b) => {
    wc.sendInputEvent({ type: 'mouseMove', x: a.x, y: a.y });
    wc.sendInputEvent({ type: 'mouseDown', x: a.x, y: a.y, button: 'left', clickCount: 1 });
    for (let i = 1; i <= 8; i++) {
      wc.sendInputEvent({ type: 'mouseMove', x: Math.round(a.x + ((b.x - a.x) * i) / 8), y: Math.round(a.y + ((b.y - a.y) * i) / 8), button: 'left', modifiers: ['leftButtonDown'] });
      await sleep(25);
    }
    wc.sendInputEvent({ type: 'mouseUp', x: b.x, y: b.y, button: 'left', clickCount: 1 });
    await sleep(150);
  };
  const shot = async (name) => { const img = await wc.capturePage(); fs.writeFileSync(path.join(out, name + '.png'), img.toPNG()); };

  let exitCode = 1;
  try {
    if (wc.isLoading()) await new Promise((r) => wc.once('did-finish-load', r));
    for (let i = 0; i < 100 && !(await js('!!(window.ML && ML.app && ML.app.doc && ML.app.L)')); i++) await sleep(100);
    wc.focus();
    await js(`
      window.__t = {
        center(id) { const t = ML.app.L.nodes.get(id), r = document.getElementById('stage').getBoundingClientRect(), v = ML.app.view;
          return { x: Math.round(r.left + v.x + (t.x + t.w / 2) * v.zoom), y: Math.round(r.top + v.y + (t.y + t.h / 2) * v.zoom) }; },
        byText(txt) { let f = null; ML.model.walkDoc(ML.app.doc, (n) => { if (n.text === txt && !f) f = n; }); return f; },
        at(txt) { const n = this.byText(txt); return n ? this.center(n.id) : null; },
        parentOf(txt) { const n = this.byText(txt); const f = n && ML.model.find(ML.app.doc, n.id); return f && f.parent ? f.parent.text : null; },
        count() { return ML.model.stats(ML.app.doc).nodes; },
        domOk() { const h = document.getElementById('viewport').innerHTML; return document.querySelectorAll('#viewport .node').length === ML.app.L.list.length && !/NaN|undefined/.test(h); },
      };
      ML.platform.fullscreen = () => {};
      ML.app.fit(false); true`);

    // ---- boot ----
    check('boot: welcome map loaded', (await js('ML.app.doc.root.text')) === 'Welcome to Mindloom');
    check('boot: desktop bridge present', await js('!!window.mindloom && ML.platform.isElectron && !!window.mindloom.version'));
    check('boot: all topics drawn', await js('__t.domOk()'));
    await sleep(300);
    await shot('01-welcome');

    // ---- real keyboard + mouse editing ----
    const n0 = await js('__t.count()');
    await click(await js('__t.at("Create")'));
    check('click selects topic', (await js('ML.app.primaryNode() && ML.app.primaryNode().text')) === 'Create');
    await key('Tab');
    check('Tab opens editor on new subtopic', await js('!!ML.app.editing'));
    await type('Alpha');
    await key('Enter');
    check('typed text is committed', (await js('__t.parentOf("Alpha")')) === 'Create');
    await key('Enter');
    await type('Beta');
    await key('Enter');
    check('Enter adds a sibling topic', (await js('__t.parentOf("Beta")')) === 'Create' && (await js('__t.count()')) === n0 + 2);
    await type('Z');
    await type('ed');
    await key('Enter');
    check('typing on a selected topic renames it', await js('!!__t.byText("Zed") && !__t.byText("Beta")'));
    await key('F2');
    await type('Gamma');
    await key('Escape');
    check('Escape cancels an edit', await js('!!__t.byText("Zed") && !__t.byText("Gamma") && !ML.app.editing'));
    await key('Tab', ['shift']);
    await type('Wrapper');
    await key('Enter');
    check('Shift+Tab inserts a parent topic', (await js('__t.parentOf("Zed")')) === 'Wrapper' && (await js('__t.parentOf("Wrapper")')) === 'Create');
    await js('ML.app.run("undo"); ML.app.run("undo"); true');
    check('undo restores previous structure', (await js('__t.parentOf("Zed")')) === 'Create' && !(await js('!!__t.byText("Wrapper")')));
    await js('ML.app.run("redo"); ML.app.run("redo"); true');
    check('redo re-applies it', (await js('__t.parentOf("Zed")')) === 'Wrapper');
    await js('ML.app.run("undo"); ML.app.run("undo"); ML.app.fit(false); true');

    // arrow-key navigation moves the selection
    await click(await js('__t.at("Welcome to Mindloom")'));
    await key('Right');
    const nav = await js('ML.app.primaryNode().text');
    check('arrow keys navigate', nav !== 'Welcome to Mindloom', nav);

    // delete
    await click(await js('__t.at("Zed")'));
    await key('Delete');
    check('Delete removes the topic', !(await js('!!__t.byText("Zed")')));
    await js('ML.app.select(ML.app.doc.root.id); true');
    await key('Delete');
    check('central topic cannot be deleted', (await js('ML.app.doc.root.text')) === 'Welcome to Mindloom');

    // ---- drag & drop ----
    await js('ML.app.fit(false); true');
    await drag(await js('__t.at("Alpha")'), await js('__t.at("Share")'));
    check('drag a topic onto another to move it', (await js('__t.parentOf("Alpha")')) === 'Share', await js('__t.parentOf("Alpha")'));
    await js('ML.app.fit(false); true');
    const stageBox = await js('(() => { const r = document.getElementById("stage").getBoundingClientRect(); return { x: Math.round(r.left + 40), y: Math.round(r.top + 80) }; })()');
    await drag(await js('__t.at("Alpha")'), stageBox);
    check('drag to empty space makes a floating topic', await js('ML.app.doc.floating.some((f) => f.text === "Alpha")'));
    await js('ML.app.fit(false); true');
    await drag(await js('__t.at("Alpha")'), await js('__t.at("Create")'));
    check('floating topic can be re-attached', (await js('__t.parentOf("Alpha")')) === 'Create' && (await js('ML.app.doc.floating.length')) === 0);

    // ---- double-click canvas → floating topic ----
    await js('ML.app.fit(false); true');
    const before = await js('ML.app.doc.floating.length');
    await click(stageBox, 2);
    await type('Loose idea');
    await key('Enter');
    check('double-click canvas creates a floating topic', (await js('ML.app.doc.floating.length')) === before + 1 && (await js('!!__t.byText("Loose idea")')));

    // ---- double-click topic → edit ----
    await js('ML.app.fit(false); true');
    await click(await js('__t.at("Alpha")'), 2);
    check('double-click topic starts editing', await js('!!ML.app.editing && ML.model.getNode(ML.app.doc, ML.app.editing.id).text === "Alpha"'));
    await key('Escape');

    // ---- context menu ----
    const pa = await js('__t.at("Alpha")');
    wc.sendInputEvent({ type: 'mouseDown', x: pa.x, y: pa.y, button: 'right', clickCount: 1 });
    wc.sendInputEvent({ type: 'mouseUp', x: pa.x, y: pa.y, button: 'right', clickCount: 1 });
    wc.sendInputEvent({ type: 'contextMenu', x: pa.x, y: pa.y });
    await sleep(200);
    const menuShown = await js('!document.getElementById("ctxmenu").hidden && document.querySelectorAll("#ctxmenu .mi").length');
    check('right-click opens the context menu', menuShown > 10, menuShown);
    await js('ML.app.hideMenu(); true');

    // ---- features through commands ----
    await js(`(() => { const a = __t.byText("Alpha"); ML.app.select(a.id);
      ML.app.run("marker", "p1"); ML.app.run("marker", "🔥"); ML.app.run("task"); ML.app.run("boundary");
      ML.app.run("style", { fill: "#ffd43b", shape: "hexagon", bold: true, fontSize: 20 });
      a.note = "A note"; a.link = "https://example.com"; a.tags = ["tag one"]; ML.app.commit();
      ML.model.addRelation(ML.app.doc, a.id, __t.byText("Share").id, "feeds"); ML.app.commit(); })(); true`);
    check('markers, task, note, link, tags, boundary, style render', await js(`(() => { const a = __t.byText("Alpha"); const g = document.querySelector('.node[data-id="' + a.id + '"]');
      return !!g && g.querySelectorAll('.icon').length === 5 && !!g.querySelector('polygon.shape') && !!g.querySelector('.tag') && !!document.querySelector('.boundary') && document.querySelectorAll('.relation').length === 2 && __t.domOk(); })()`));
    await js('ML.app.select(__t.byText("Create").id); ML.app.run("toggleCollapse"); true');
    check('collapse hides the branch', await js('!ML.app.L.nodes.has(__t.byText("Alpha").id) && !!document.querySelector(".badge")'));
    await js('ML.app.run("toggleCollapse"); ML.app.select(__t.byText("Style").id); ML.app.run("focus"); true');
    check('focus mode shows only one branch', await js('ML.app.L.list.length === 5 && !document.getElementById("focus-bar").hidden'));
    await js('ML.app.run("focus"); true');
    check('leaving focus mode restores the map', await js('ML.app.L.list.length === __t.count()'));
    await js('ML.app.run("find"); const i = document.getElementById("search-input"); i.value = "theme"; i.dispatchEvent(new Event("input")); true');
    check('search finds topics', (await js('ML.app.matches.length')) >= 1);
    await js('document.getElementById("search-close").click(); true');
    await js('ML.app.select(__t.byText("Organise").id); ML.app.run("copy"); ML.app.select(__t.byText("Share").id); ML.app.run("paste"); true');
    check('copy / paste duplicates a branch', await js('__t.byText("Share").children.some((c) => c.text === "Organise" && c.children.length === 4)'));
    await js('ML.app.run("undo"); true');
    await shot('02-decorated');

    // ---- inspector panels ----
    for (const tab of ['topic', 'map', 'markers', 'notes']) {
      await js(`ML.app.select(__t.byText("Alpha").id); ML.app.emit("show-tab", "${tab}"); true`);
      await sleep(80);
      const len = await js('document.getElementById("insp-body").children.length');
      check(`inspector tab "${tab}" renders`, len > 0, len);
    }
    await js('ML.app.emit("show-tab", "map"); true');
    await sleep(250);
    check('map tab shows style, theme and structure previews', await js('document.querySelectorAll("#insp-body [data-mapstyle]").length === ML.themes.MAP_STYLES.length && document.querySelectorAll("#insp-body [data-theme]").length === ML.themes.THEMES.length && document.querySelectorAll("#insp-body [data-layout]").length === ML.themes.LAYOUTS.length && [...document.querySelectorAll("#insp-body img")].every((i) => i.complete && i.naturalWidth > 0)'));
    await js('document.querySelector(\'#insp-body [data-theme="ocean"]\').click(); true');
    check('clicking a theme card applies it', (await js('ML.app.doc.settings.theme')) === 'ocean');
    await js('document.querySelector(\'#insp-body [data-mapstyle="org"]\').click(); true');
    check('clicking a map style applies it', (await js('ML.app.doc.settings.layout')) === 'down' && (await js('ML.app.doc.settings.lineStyle')) === 'elbow');
    await shot('03-map-panel');

    // ---- every structure × theme × line style renders ----
    const combos = await js(`(() => { const bad = []; const T = ML.themes; let n = 0;
      for (const l of T.LAYOUTS) for (const th of T.THEMES) { const ls = T.LINE_STYLES[n % T.LINE_STYLES.length].id; const sh = T.SHAPES[n % T.SHAPES.length].id; n++;
        Object.assign(ML.app.doc.settings, { layout: l.id, theme: th.id, lineStyle: ls, pattern: T.PATTERNS[n % T.PATTERNS.length].id, shapes: { root: 'rounded', main: sh, sub: T.SHAPES[(n + 3) % T.SHAPES.length].id } });
        try { ML.app.render(); if (!__t.domOk()) bad.push(l.id + '/' + th.id); } catch (e) { bad.push(l.id + '/' + th.id + ': ' + e.message); } }
      return { n, bad }; })()`);
    check(`all ${combos.n} structure × theme combinations render`, combos.bad.length === 0, combos.bad.join(', '));
    const looks = [['mindmap', 'fresh', 'tapered', 'dots'], ['right', 'business', 'rounded', 'grid'], ['down', 'ocean', 'elbow', 'none'], ['tree', 'lavender', 'rounded', 'lines'],
      ['timeline', 'candy', 'rounded', 'none'], ['fishbone', 'sunset', 'straight', 'none'], ['radial', 'midnight', 'curve', 'isometric'], ['mindmap', 'neon', 'curve', 'grid'], ['left', 'chalk', 'curve', 'cross']];
    let li = 4;
    for (const [layout, theme, lineStyle, pattern] of looks) {
      await js(`Object.assign(ML.app.doc.settings, { layout: "${layout}", theme: "${theme}", lineStyle: "${lineStyle}", pattern: "${pattern}", shapes: { root: 'rounded', main: 'rounded', sub: '${layout === 'tree' ? 'none' : 'rounded'}' } }); ML.app.select([]); ML.app.render(); ML.app.fit(false); true`);
      await sleep(150);
      await shot(`${String(li++).padStart(2, '0')}-${layout}-${theme}`);
    }
    await js('Object.assign(ML.app.doc.settings, { layout: "mindmap", theme: "fresh", lineStyle: "tapered", pattern: "dots" }); ML.app.commit(); ML.app.fit(false); true');

    // ---- dialogs ----
    await js('ML.app.run("templates"); true');
    await sleep(500);
    check('template gallery lists every template', (await js('document.querySelectorAll("#modal-root .tpl").length')) === (await js('ML.templates.TEMPLATES.length')));
    await shot('13-templates');
    await key('Escape');
    check('Escape closes dialogs', (await js('document.querySelectorAll("#modal-root .overlay").length')) === 0);
    await js('ML.app.run("shortcuts"); true');
    check('shortcuts dialog opens', (await js('document.querySelectorAll("#modal-root .kbd-table tr").length')) > 20);
    await key('Escape');
    await js('ML.app.run("exportMenu"); true');
    check('export menu opens', (await js('document.querySelectorAll("#ctxmenu .mi").length')) >= 9);
    await js('ML.app.hideMenu(); true');

    // ---- presentation ----
    await js('ML.app.run("present"); true');
    await sleep(500);
    await key('Right');
    await sleep(450);
    check('presentation mode steps through branches', await js('!!ML.app.present && ML.app.present.i === 1 && document.body.classList.contains("presenting")'));
    await shot('14-presentation');
    await key('Escape');
    await sleep(400);
    check('Escape leaves presentation mode', await js('!ML.app.present && !document.body.classList.contains("presenting")'));

    // ---- exports ----
    const svgInfo = await js(`(() => { const d = ML.app.exportData(); const doc = new DOMParser().parseFromString(d.svg, 'image/svg+xml');
      return { ok: !doc.querySelector('parsererror'), w: d.width, h: d.height, svg: d.svg, nodes: doc.querySelectorAll('.node').length }; })()`);
    check('SVG export is valid XML with every topic', svgInfo.ok && svgInfo.nodes === (await js('ML.app.L.list.length')), `${svgInfo.w}x${svgInfo.h}`);
    fs.writeFileSync(path.join(out, 'export.svg'), svgInfo.svg);
    const png = await js(`ML.app.renderPNG(2).then(async (b) => { const u = new Uint8Array(await b.arrayBuffer()); let s = ''; for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode.apply(null, u.subarray(i, i + 0x8000)); return btoa(s); })`);
    const pngBuf = Buffer.from(png, 'base64');
    fs.writeFileSync(path.join(out, 'export.png'), pngBuf);
    check('PNG export produces a real image', pngBuf.length > 20000 && pngBuf.slice(1, 4).toString() === 'PNG' && pngBuf.readUInt32BE(16) === Math.round(svgInfo.w * 2), `${pngBuf.length} bytes, ${pngBuf.readUInt32BE(16)}x${pngBuf.readUInt32BE(20)}`);
    const pdf = await svgToPdf(svgInfo.svg, svgInfo.w, svgInfo.h);
    fs.writeFileSync(path.join(out, 'export.pdf'), pdf);
    check('PDF export produces a PDF', pdf.length > 5000 && pdf.slice(0, 5).toString() === '%PDF-', `${pdf.length} bytes`);
    const text = await js('({ md: ML.io.toMarkdown(ML.app.doc), opml: ML.io.toOPML(ML.app.doc), mm: ML.io.toFreeMind(ML.app.doc), txt: ML.io.toText(ML.app.doc) })');
    for (const [ext, body] of Object.entries(text)) fs.writeFileSync(path.join(out, 'export.' + ext), body);
    check('Markdown / OPML / FreeMind / text exports have content', text.md.startsWith('# Welcome to Mindloom') && text.opml.includes('<opml') && text.mm.includes('<map') && text.txt.split('\n').length > 20);

    // ---- save to disk, change, reopen ----
    const file = path.join(out, 'roundtrip.mindloom');
    const saved = await js(`(async () => { ML.app.path = ${JSON.stringify(file)}; const ok = await ML.app.save(false); return { ok, dirty: ML.app.dirty, n: __t.count() }; })()`);
    check('save writes the .mindloom file', saved.ok && !saved.dirty && fs.existsSync(file) && JSON.parse(fs.readFileSync(file, 'utf8')).app === 'mindloom');
    await js('ML.app.setDoc(ML.templates.build("blank")); true');
    await js(`ML.app.run("openPath", ${JSON.stringify(file)})`);
    await sleep(500);
    const re = await js('({ n: __t.count(), root: ML.app.doc.root.text, alpha: !!__t.byText("Alpha"), rel: ML.app.doc.relations.length, fl: ML.app.doc.floating.length, path: ML.app.path, dirty: ML.app.dirty, style: __t.byText("Alpha").style })');
    check('reopening restores the whole map', re.n === saved.n && re.alpha && re.rel === 2 && re.fl === 1 && re.path === file && !re.dirty && re.style && re.style.shape === 'hexagon', JSON.stringify(re));
    check('recent files list is updated', (await js('window.mindloom.getRecent()')).includes(file));

    // ---- import other formats from disk through the real file path ----
    for (const ext of ['md', 'opml', 'mm', 'txt']) {
      await js('ML.app.setDirty(false); true');
      await js(`ML.app.run("openPath", ${JSON.stringify(path.join(out, 'export.' + ext))})`);
      await sleep(350);
      const r = await js('({ root: ML.app.doc.root.text, kids: ML.app.doc.root.children.length, floating: ML.app.doc.floating.map((f) => f.text).join(), n: __t.count(), placed: ML.app.doc.floating.every((f) => !f._place && f.y > ML.app.L.nodes.get(ML.app.doc.root.id).y), dom: __t.domOk() })');
      check(`import .${ext} file`, r.root === 'Welcome to Mindloom' && r.kids === 5 && r.floating === 'Loose idea' && r.n === saved.n && r.placed && r.dom, JSON.stringify(r));
    }

    // ---- every template opens ----
    const tpl = await js(`(() => { const bad = []; for (const t of ML.templates.TEMPLATES) { try { ML.app.setDoc(ML.templates.build(t.id)); if (!__t.domOk()) bad.push(t.id); } catch (e) { bad.push(t.id + ': ' + e.message); } } return bad; })()`);
    check('every template opens and renders', tpl.length === 0, tpl.join(', '));

    // ---- AI assistant fails gracefully without a key ----
    const ai = await js('ML.platform.aiKey("has").then((h) => h ? "has-key" : ML.platform.ai({ mode: "generate", topic: "x" }).then(() => "unexpected", (e) => e.message))');
    check('AI assistant asks for a key instead of crashing', ai === 'has-key' || /API key/i.test(ai), ai);

    // ---- large map performance ----
    const perf = await js(`(() => { const d = ML.model.createDoc('Big'); let n = 0;
      for (let i = 0; i < 8; i++) { const a = ML.model.addChild(d, d.root.id, 'Branch ' + i); for (let j = 0; j < 8; j++) { const b = ML.model.addChild(d, a.id, 'Topic ' + i + '.' + j); for (let k = 0; k < 8; k++) { const c = ML.model.addChild(d, b.id, 'Idea ' + (n++)); if (k < 2) ML.model.addChild(d, c.id, 'Detail'); } } }
      ML.app.setDoc(d); const t0 = performance.now(); for (let i = 0; i < 5; i++) ML.app.render(); return { nodes: __t.count(), ms: (performance.now() - t0) / 5, dom: __t.domOk() }; })()`);
    check(`large map (${perf.nodes} topics) renders quickly`, perf.dom && perf.ms < 400, `${perf.ms.toFixed(1)} ms per render`);

    await sleep(200);
    check('no console errors during the run', errors.length === 0, errors.slice(0, 5).join(' | '));
    exitCode = results.every((r) => r.ok) ? 0 : 1;
  } catch (err) {
    check('self test completed', false, err && err.stack ? err.stack : err);
    if (errors.length) console.log('console errors:', errors.slice(0, 5));
  }
  const passed = results.filter((r) => r.ok).length;
  fs.writeFileSync(path.join(out, 'results.json'), JSON.stringify({ passed, failed: results.length - passed, version: app.getVersion(), packaged: app.isPackaged, platform: process.platform, arch: process.arch, electron: process.versions.electron, results }, null, 2));
  console.log(`\nMindloom self test: ${passed}/${results.length} passed${exitCode ? ' — FAILED' : ''}`);
  app.exit(exitCode);
};
