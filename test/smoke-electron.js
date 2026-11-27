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
        center(id) { ML.app.settleView(); const t = ML.app.L.nodes.get(id), r = document.getElementById('stage').getBoundingClientRect(), v = ML.app.view;
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
    // helpers: world position of a topic, and the pixel that is N world units away from a topic
    await js(`__t.box = (txt) => { const t = ML.app.L.nodes.get(__t.byText(txt).id); return { x: t.x, y: t.y, w: t.w, h: t.h, side: t.side }; };
      __t.screen = (wx, wy) => { ML.app.settleView(); const r = document.getElementById('stage').getBoundingClientRect(), v = ML.app.view; return { x: Math.round(r.left + v.x + wx * v.zoom), y: Math.round(r.top + v.y + wy * v.zoom) }; };
      __t.edgeTo = (txt) => ML.app.L.edges.some((e) => e.to === __t.byText(txt).id);
      __t.positions = () => Object.fromEntries(ML.app.L.list.map((t) => [t.node.text, [Math.round(t.x), Math.round(t.y)]]));
      // nearest pointer position to (topic centre + dx,dy) that is clear of every other topic and inside the window
      __t.emptyNear = (txt, dx, dy) => {
        ML.app.settleView();
        const n = __t.byText(txt), t = ML.app.L.nodes.get(n.id), skip = new Set();
        ML.model.walk(n, (x) => { skip.add(x.id); });
        const cx = t.x + t.w / 2, cy = t.y + t.h / 2;
        const r = document.getElementById('stage').getBoundingClientRect(), v = ML.app.view;
        const free = (wx, wy) => {
          const sx = v.x + wx * v.zoom, sy = v.y + wy * v.zoom;
          if (sx < 30 || sy < 30 || sx > r.width - 260 || sy > r.height - 30) return false;
          return ML.app.L.list.every((o) => skip.has(o.id) || wx < o.x - 28 || wx > o.x + o.w + 28 || wy < o.y - 28 || wy > o.y + o.h + 28);
        };
        for (let rad = 0; rad < 900; rad += 12) for (let a = 0; a < 24; a++) {
          const wx = cx + dx + Math.cos((a / 24) * Math.PI * 2) * rad, wy = cy + dy + Math.sin((a / 24) * Math.PI * 2) * rad;
          if (free(wx, wy)) return { dx: wx - cx, dy: wy - cy, ...__t.screen(wx, wy) };
        }
        return null;
      }; true`);
    const zoomTo1 = 'ML.app.view.zoom = 1; ML.app.centerOn(__t.byText("Create").id, false); true';
    await js(zoomTo1);

    // 1. drag a subtopic into empty space: it stays connected, its siblings do not move
    const posBefore = await js('__t.positions()');
    const ab = await js('__t.box("Alpha")');
    const from = await js('__t.at("Alpha")');
    const spot1 = await js('__t.emptyNear("Alpha", 160, 70)');
    await drag(from, spot1);
    const posAfter = await js('__t.positions()');
    const ab2 = await js('__t.box("Alpha")');
    const others = Object.keys(posBefore).filter((k) => k !== 'Alpha' && JSON.stringify(posBefore[k]) !== JSON.stringify(posAfter[k]));
    check('drag to empty space keeps the topic connected to its parent', (await js('__t.parentOf("Alpha")')) === 'Create' && (await js('__t.edgeTo("Alpha")')) && (await js('ML.app.doc.floating.length')) === 0);
    check('the dragged topic lands where it was dropped', Math.abs(ab2.x - ab.x - spot1.dx) < 3 && Math.abs(ab2.y - ab.y - spot1.dy) < 3, `moved ${Math.round(ab2.x - ab.x)},${Math.round(ab2.y - ab.y)} wanted ${Math.round(spot1.dx)},${Math.round(spot1.dy)}`);
    check('no other topic moves or swaps place', others.length === 0, others.join(', '));
    check('the move is saved with the map', await js('!!__t.byText("Alpha").offset && __t.byText("Alpha").offset.l === "mindmap"'));

    // 2. its subtopics travel with it
    await js('ML.app.select(__t.byText("Alpha").id); ML.app.run("addChild"); ML.app.cancelEdit(); true');
    await js('ML.model.getNode(ML.app.doc, ML.app.primary()).text = "Alpha child"; ML.app.commit(); true');
    const kidBefore = await js('__t.box("Alpha child")');
    const from2 = await js('__t.at("Alpha")');
    const spot2 = await js('__t.emptyNear("Alpha", 60, 60)');
    await drag(from2, spot2);
    const kidAfter = await js('__t.box("Alpha child")');
    check('a moved topic carries its whole branch', Math.abs(kidAfter.x - kidBefore.x - spot2.dx) < 3 && Math.abs(kidAfter.y - kidBefore.y - spot2.dy) < 3, `${Math.round(kidAfter.x - kidBefore.x)},${Math.round(kidAfter.y - kidBefore.y)}`);

    await js('ML.app.select([]); true'); await sleep(100); await shot('15-free-move');
    // dragged to the far side of its parent: the connector re-routes cleanly
    const cr = await js('__t.box("Create")');
    const crossAt = await js(`__t.emptyNear("Alpha", ${cr.x - 160} - (__t.box("Alpha").x + __t.box("Alpha").w / 2), 0)`);
    await drag(await js('__t.at("Alpha")'), crossAt);
    await js('ML.app.select([]); true'); await sleep(100); await shot('16-crossed-parent');
    await js('ML.app.run("undo"); true');
    // 3. undo / redo the move
    await js('ML.app.run("undo"); true');
    const undone = await js('__t.box("Alpha")');
    check('undo restores the previous position', Math.abs(undone.x - ab2.x) < 2 && Math.abs(undone.y - ab2.y) < 2);
    await js('ML.app.run("redo"); true');

    // 4. drag it back near its home slot: it snaps home and the offset is cleared
    const home = await js('(() => { const n = __t.byText("Alpha"); const L = ML.layout.layout(ML.app.doc, { theme: ML.app.theme(), settings: ML.app.doc.settings, measure: ML.app.measure, ignoreOffsetId: n.id }); const t = L.nodes.get(n.id); return { x: t.x + t.w / 2 + 4, y: t.y + t.h / 2 - 3 }; })()');
    await drag(await js('__t.at("Alpha")'), await js(`__t.screen(${home.x}, ${home.y})`));
    check('dropping near the original slot snaps the topic back', await js('!__t.byText("Alpha").offset'), await js('JSON.stringify(__t.byText("Alpha").offset)'));

    // 5. Escape mid-drag cancels
    const esc0 = await js('__t.box("Alpha")');
    const pe = await js('__t.at("Alpha")');
    wc.sendInputEvent({ type: 'mouseMove', x: pe.x, y: pe.y });
    wc.sendInputEvent({ type: 'mouseDown', x: pe.x, y: pe.y, button: 'left', clickCount: 1 });
    for (let i = 1; i <= 6; i++) { wc.sendInputEvent({ type: 'mouseMove', x: pe.x + i * 20, y: pe.y + i * 12, button: 'left', modifiers: ['leftButtonDown'] }); await sleep(30); }
    await key('Escape');
    wc.sendInputEvent({ type: 'mouseUp', x: pe.x + 120, y: pe.y + 72, button: 'left', clickCount: 1 });
    await sleep(150);
    const esc1 = await js('__t.box("Alpha")');
    check('Escape cancels a drag', Math.abs(esc1.x - esc0.x) < 1 && Math.abs(esc1.y - esc0.y) < 1 && !(await js('!!__t.byText("Alpha").offset')));

    // 6. drop onto another topic re-parents it
    await js('ML.app.fit(false); true');
    await drag(await js('__t.at("Alpha")'), await js('__t.at("Share")'));
    check('dropping onto a topic makes it a subtopic there', (await js('__t.parentOf("Alpha")')) === 'Share');
    await js('ML.app.fit(false); true');
    await drag(await js('__t.at("Alpha")'), await js('__t.at("Create")'));
    check('and it can be moved back', (await js('__t.parentOf("Alpha")')) === 'Create');

    // 7. drop on the top edge of a sibling reorders
    await js('ML.app.view.zoom = 1; ML.app.centerOn(__t.byText("Create").id, false); true');
    const order0 = await js('__t.byText("Create").children.map((c) => c.text)');
    const tb = await js('__t.box("Tab adds a subtopic")');
    await drag(await js('__t.at("Alpha")'), await js(`__t.screen(${tb.x + tb.w / 2}, ${tb.y + 3})`));
    const order1 = await js('__t.byText("Create").children.map((c) => c.text)');
    check('dropping on a sibling edge reorders', order1[0] === 'Alpha' && order1.length === order0.length, order1.join(' | '));

    // 8. main topic dragged across the central topic switches side
    await js('ML.app.fit(false); true');
    const side0 = await js('__t.box("Share").side');
    const root = await js('__t.box("Welcome to Mindloom")');
    const sb = await js('__t.box("Share")');
    const want = (side0 === 'left' ? root.x + root.w + 140 : root.x - 140) - (sb.x + sb.w / 2);
    const spot8 = await js(`__t.emptyNear("Share", ${want}, ${root.y + root.h / 2 + 40 - (sb.y + sb.h / 2)})`);
    await drag(await js('__t.at("Share")'), spot8);
    const side1 = await js('__t.box("Share").side');
    check('a main topic dragged across the centre switches side', side1 && side1 !== side0 && (await js('__t.byText("Share").side')) === side1 && !(await js('!!__t.byText("Share").offset')), `${side0} -> ${side1}`);

    // 9. sides stay put when topics are added or removed
    const sides0 = await js('Object.fromEntries(ML.app.doc.root.children.map((c) => [c.text, ML.app.L.nodes.get(c.id).side]))');
    await js('ML.app.select(ML.app.doc.root.id); ML.app.run("addChild"); ML.app.cancelEdit(); ML.app.run("addChild"); ML.app.cancelEdit(); true');
    await js('ML.app.select(__t.byText("Organise").id); ML.app.run("deleteSel"); true');
    const sides1 = await js('Object.fromEntries(ML.app.doc.root.children.map((c) => [c.text, ML.app.L.nodes.get(c.id).side]))');
    const flipped = Object.keys(sides0).filter((k) => k in sides1 && sides0[k] !== sides1[k]);
    check('adding or deleting main topics never moves the others to the other side', flipped.length === 0, flipped.join(', '));
    await js('ML.app.run("undo"); ML.app.run("undo"); ML.app.run("undo"); true');
    check('undo brings the deleted topic back', await js('!!__t.byText("Organise")'));

    // 10. multi-select drag moves the whole selection together
    await js('ML.app.view.zoom = 1; ML.app.centerOn(__t.byText("Create").id, false); true');
    await click(await js('__t.at("Enter adds a topic")'));
    wc.sendInputEvent({ type: 'mouseDown', ...(await js('__t.at("Just start typing to rename")')), button: 'left', clickCount: 1, modifiers: ['shift'] });
    wc.sendInputEvent({ type: 'mouseUp', ...(await js('__t.at("Just start typing to rename")')), button: 'left', clickCount: 1, modifiers: ['shift'] });
    await sleep(120);
    check('shift-click adds to the selection', (await js('ML.app.sel.length')) === 2);
    // (checked after the drag below: a plain click narrows the selection)
    const m0a = await js('__t.box("Enter adds a topic")'), m0b = await js('__t.box("Just start typing to rename")');
    const pm = await js('__t.at("Enter adds a topic")');
    const spot10 = await js('__t.emptyNear("Enter adds a topic", 260, 0)');
    await drag(pm, spot10);
    const m1a = await js('__t.box("Enter adds a topic")'), m1b = await js('__t.box("Just start typing to rename")');
    check('dragging a multi-selection moves every selected topic', Math.abs(m1a.x - m0a.x - spot10.dx) < 3 && Math.abs(m1b.x - m0b.x - spot10.dx) < 3 && Math.abs(m1b.y - m0b.y - spot10.dy) < 3,
      `${Math.round(m1a.x - m0a.x)},${Math.round(m1b.x - m0b.x)} wanted ${Math.round(spot10.dx)}; parents ${await js('__t.parentOf("Enter adds a topic")')}`);
    await click(await js('__t.at("Enter adds a topic")'));
    check('a plain click on one topic of a multi-selection selects just that topic', (await js('ML.app.sel.length')) === 1);
    await js('ML.app.run("resetAllPositions"); true');
    check('"Reset all hand-placed topics" clears every manual position', await js('(() => { let n = 0; ML.model.walkDoc(ML.app.doc, (x) => { if (x.offset) n++; }); return n === 0; })()'));

    // 11. keyboard nudge
    await js('ML.app.select(__t.byText("Alpha").id); true');
    const n0b = await js('__t.box("Alpha")');
    await key('Right', ['shift', 'alt']); await key('Right', ['shift', 'alt']); await key('Down', ['shift', 'alt']);
    const n1b = await js('__t.box("Alpha")');
    check('Shift+Alt+arrows nudge a topic', Math.round(n1b.x - n0b.x) === 20 && Math.round(n1b.y - n0b.y) === 10, `${n1b.x - n0b.x},${n1b.y - n0b.y}`);
    await js('ML.app.run("resetPosition"); true');

    // 12. free positioning in every structure that supports it, and a clear message where it doesn't
    for (const lay of ['right', 'down', 'tree', 'radial', 'timeline', 'fishbone']) {
      await js(`ML.app.setSettings({ layout: "${lay}" }); ML.app.fit(false); true`);
      const target = 'Organise';
      const b0 = await js(`__t.box("${target}")`);
      const p0 = await js(`__t.at("${target}")`);
      await drag(p0, await js(`__t.emptyNear("${target}", 70, 50)`));
      const b1 = await js(`__t.box("${target}")`);
      const movedIt = Math.abs(b1.x - b0.x) > 5 || Math.abs(b1.y - b0.y) > 5;
      const connected = (await js(`__t.parentOf("${target}")`)) === 'Welcome to Mindloom' && (await js(`__t.edgeTo("${target}")`)) && (await js('__t.domOk()'));
      if (lay === 'timeline' || lay === 'fishbone') check(`${lay}: main topics keep their slot and stay connected`, !movedIt && connected);
      else check(`${lay}: a dragged topic moves and stays connected`, movedIt && connected);
      if (lay === 'timeline') {
        const s0 = await js('__t.box("Drag topics anywhere, they stay connected")');
        const sp = await js('__t.at("Drag topics anywhere, they stay connected")');
        await drag(sp, await js('__t.emptyNear("Drag topics anywhere, they stay connected", 60, 40)'));
        const s1 = await js('__t.box("Drag topics anywhere, they stay connected")');
        check('timeline: subtopics can be moved freely', Math.abs(s1.x - s0.x) > 5 && (await js('__t.parentOf("Drag topics anywhere, they stay connected")')) === 'Organise');
      }
    }
    await js('ML.app.run("resetAllPositions"); ML.app.setSettings({ layout: "mindmap" }); ML.app.fit(false); true');

    // 13. Shift + drag on the canvas selects with a rectangle
    await js('ML.app.view.zoom = 1; ML.app.centerOn(__t.byText("Create").id, false); ML.app.select([]); true');
    const tb1 = await js('__t.box("Tab adds a subtopic")'), tb2 = await js('__t.box("Enter adds a topic")');
    const ms = await js(`__t.screen(${Math.min(tb1.x, tb2.x) - 12}, ${tb1.y - 12})`), me = await js(`__t.screen(${Math.max(tb1.x + tb1.w, tb2.x + tb2.w) + 12}, ${tb2.y + tb2.h + 4})`);
    wc.sendInputEvent({ type: 'mouseMove', x: ms.x, y: ms.y });
    wc.sendInputEvent({ type: 'mouseDown', x: ms.x, y: ms.y, button: 'left', clickCount: 1, modifiers: ['shift'] });
    for (let i = 1; i <= 6; i++) { wc.sendInputEvent({ type: 'mouseMove', x: Math.round(ms.x + ((me.x - ms.x) * i) / 6), y: Math.round(ms.y + ((me.y - ms.y) * i) / 6), button: 'left', modifiers: ['leftButtonDown', 'shift'] }); await sleep(25); }
    wc.sendInputEvent({ type: 'mouseUp', x: me.x, y: me.y, button: 'left', clickCount: 1, modifiers: ['shift'] });
    await sleep(150);
    const marq = await js('ML.app.sel.map((id) => ML.model.getNode(ML.app.doc, id).text).sort()');
    check('Shift + drag on the canvas selects topics in the rectangle', marq.includes('Tab adds a subtopic') && marq.includes('Enter adds a topic') && !marq.includes('Create'), marq.join(' | '));

    // 14. detach and re-attach floating topics
    await js('ML.app.select(__t.byText("Alpha").id); ML.app.run("detach"); ML.app.fit(false); true');
    check('"Detach as floating topic" still works', await js('ML.app.doc.floating.some((f) => f.text === "Alpha")'));
    const fl0 = await js('__t.box("Alpha")');
    const fp = await js('__t.at("Alpha")');
    await drag(fp, await js('__t.emptyNear("Alpha", 40, 40)'));
    check('floating topics move freely', (await js('__t.box("Alpha").x')) !== fl0.x && (await js('ML.app.doc.floating.length')) === 1);
    await js('ML.app.fit(false); true');
    await drag(await js('__t.at("Alpha")'), await js('__t.at("Create")'));
    check('floating topic can be re-attached', (await js('__t.parentOf("Alpha")')) === 'Create' && (await js('ML.app.doc.floating.length')) === 0);
    const stageBox = await js('(() => { const r = document.getElementById("stage").getBoundingClientRect(); return { x: Math.round(r.left + 40), y: Math.round(r.top + 80) }; })()');

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
    // reshape a relationship by dragging its curve
    await js('ML.app.fit(false); true');
    const relPt = await js(`(() => { const r = ML.app.doc.relations.find((x) => x.label === 'feeds'); const a = ML.app.L.nodes.get(r.from), b = ML.app.L.nodes.get(r.to);
      const p = ML.layout.relationPath(a, b, r); const s = document.getElementById('stage').getBoundingClientRect(), v = ML.app.view;
      return { x: Math.round(s.left + v.x + p.mid.x * v.zoom), y: Math.round(s.top + v.y + p.mid.y * v.zoom), bend: r.bend == null ? null : r.bend }; })()`);
    await drag(relPt, { x: relPt.x, y: relPt.y + 60 });
    const bend1 = await js('ML.app.doc.relations.find((x) => x.label === "feeds").bend');
    check('dragging a relationship curve reshapes it', bend1 != null && bend1 !== relPt.bend && (await js('ML.app.selRel === ML.app.doc.relations.find((x) => x.label === "feeds").id')), String(bend1));
    await js('ML.app.select(__t.byText("Create").id); ML.app.run("toggleCollapse"); true');
    check('collapse hides the branch', await js('!ML.app.L.nodes.has(__t.byText("Alpha").id) && !!document.querySelector(".badge")'));
    await js('ML.app.run("toggleCollapse"); ML.app.select(__t.byText("Style").id); ML.app.run("focus"); true');
    check('focus mode shows only one branch', await js('ML.app.L.list.length === 5 && !document.getElementById("focus-bar").hidden'));
    await js('ML.app.run("focus"); true');
    check('leaving focus mode restores the map', await js('ML.app.L.list.length === __t.count()'));
    await js('ML.app.run("find"); const i = document.getElementById("search-input"); i.value = "theme"; i.dispatchEvent(new Event("input")); true');
    check('search finds topics', (await js('ML.app.matches.length')) >= 1);
    await js('document.getElementById("search-close").click(); true');

    // ---- v1.2: due dates, find & replace, numbering, CSV / HTML ----
    await js('(() => { const a = __t.byText("Alpha"); a.due = "2000-01-05"; ML.app.commit(); })(); true');
    check('a past due date renders as a red overdue chip', await js(`(() => { const a = __t.byText("Alpha"); const g = document.querySelector('.node[data-id="' + a.id + '"]'); return !!g && !!g.querySelector('.tag.due.overdue') && ML.model.stats(ML.app.doc).overdue >= 1 && __t.domOk(); })()`));
    await js('ML.app.select(__t.byText("Alpha").id); ML.app.emit("show-tab", "topic"); true');
    await sleep(120);
    check('the inspector shows a due-date field with the date in it', await js('(() => { const f = document.querySelector(\'#insp-body [data-field="due"]\'); return !!f && f.value === "2000-01-05"; })()'));
    await js('ML.app.run("find"); const si = document.getElementById("search-input"); si.value = "Alpha"; si.dispatchEvent(new Event("input")); document.getElementById("replace-input").value = "Alpine"; document.getElementById("replace-all").click(); true');
    await sleep(120);
    check('find & replace renames every matching topic', await js('!!__t.byText("Alpine") && !__t.byText("Alpha")'));
    await js('document.getElementById("search-close").click(); ML.app.run("undo"); true');
    check('find & replace is one undo step', await js('!!__t.byText("Alpha") && !__t.byText("Alpine")'));
    await js('ML.app.doc.settings.numbering = "on"; ML.app.commit(); true');
    check('numbering shows outline numbers on topics', await js(`(() => { const g = document.querySelector('.node[data-id="' + __t.byText("Create").id + '"]'); return !!g && /^1\\s/.test(g.querySelector('.label').textContent.trim()) && __t.byText("Create").text === "Create" && __t.domOk(); })()`));
    await js('ML.app.doc.settings.numbering = "off"; ML.app.commit(); true');
    check('CSV and HTML exports carry the map', await js(`(() => { const csv = ML.io.toCSV(ML.app.doc); const back = ML.io.fromCSV(csv); const html = ML.io.toHTML(ML.app.doc, ML.app.exportData().svg);
      return csv.startsWith('Topic,Path,Depth') && back.root.text === ML.app.doc.root.text && back.root.children.length === ML.app.doc.root.children.length && html.startsWith('<!doctype html>') && html.includes('<svg'); })()`));
    await js('ML.app.select(__t.byText("Organise").id); ML.app.run("copy"); ML.app.select(__t.byText("Share").id); ML.app.run("paste"); true');
    check('copy / paste duplicates a branch', await js('__t.byText("Share").children.some((c) => c.text === "Organise" && c.children.length === 4)'));
    await js('ML.app.run("undo"); true');
    await shot('02-decorated');

    // ---- real menu commands, clipboard and native text undo ----
    const kids0 = await js('__t.byText("Organise").children.length');
    await js('ML.app.select(__t.byText("Organise").children[0].id); document.getElementById("stage").focus(); true');
    wc.copy();
    await sleep(150);
    await js('ML.app.select(__t.byText("Organise").id); document.getElementById("stage").focus(); true');
    wc.paste();
    await sleep(250);
    check('Edit ▸ Copy / Paste (real clipboard) duplicates a topic', (await js('__t.byText("Organise").children.length')) === kids0 + 1);
    win.webContents.send('menu', 'undo');
    await sleep(200);
    check('Edit ▸ Undo from the menu undoes the paste', (await js('__t.byText("Organise").children.length')) === kids0);
    win.webContents.send('menu', 'redo');
    await sleep(200);
    check('Edit ▸ Redo from the menu re-applies it', (await js('__t.byText("Organise").children.length')) === kids0 + 1);
    win.webContents.send('menu', 'undo');
    await sleep(150);
    // text copied in another app pastes as a branch
    require('electron').clipboard.writeText('Plan\n\tResearch\n\tBudget\nShip');
    await js('ML.app.select(__t.byText("Share").id); document.getElementById("stage").focus(); true');
    wc.paste();
    await sleep(250);
    check('pasting indented text from another app builds topics', await js('(() => { const p = __t.byText("Plan"); return !!p && p.children.length === 2 && !!__t.byText("Ship") && __t.parentOf("Ship") === "Share"; })()'));
    win.webContents.send('menu', 'undo');
    await sleep(150);
    // undo inside the text editor undoes typing, not the map
    const histLen = await js('ML.app.history.idx');
    await js('ML.app.select(__t.byText("Images").id); true');
    await key('F2');
    await type('Pictures');
    win.webContents.send('menu', 'undo');
    await sleep(150);
    const edVal = await js('document.getElementById("editor").value');
    check('Undo while editing text undoes the typing only', edVal !== 'Pictures' && (await js('ML.app.history.idx')) === histLen && (await js('!!ML.app.editing')), JSON.stringify(edVal));
    await key('Escape');

    // ---- copy / paste style ----
    await js('ML.app.select(__t.byText("Alpha").id); ML.app.run("copyStyle"); ML.app.select(__t.byText("Images").id); ML.app.run("pasteStyle"); true');
    check('copy style / paste style (format painter)', await js('__t.byText("Images").style && __t.byText("Images").style.shape === "hexagon" && __t.byText("Images").style.fill === "#ffd43b"'));
    await js('ML.app.run("undo"); true');

    // ---- images ----
    const imgOk = await js(`(async () => { const c = document.createElement('canvas'); c.width = 320; c.height = 200; const x = c.getContext('2d'); x.fillStyle = '#4c6ef5'; x.fillRect(0, 0, 320, 200);
      const blob = await new Promise((r) => c.toBlob(r, 'image/png')); const f = new File([blob], 'pic.png', { type: 'image/png' });
      await ML.app.setImageFromFile(f, __t.byText("Images").id); const n = __t.byText("Images"); const g = document.querySelector('.node[data-id="' + n.id + '"]');
      return !!n.image && /^data:image\\//.test(n.image.src) && n.image.w === 240 && !!g.querySelector('image') && __t.box("Images").h > 150; })()`);
    check('inserting an image shows it inside the topic', imgOk);
    await js('ML.app.run("undo"); true');

    // ---- floating topics: Enter adds the next one below the whole branch ----
    await js('ML.app.select(__t.byText("Loose idea").id); ML.app.run("addChild"); ML.app.cancelEdit(); ML.app.select(__t.byText("Loose idea").id); ML.app.run("addSibling"); ML.app.cancelEdit(); true');
    check('a new floating topic does not overlap the previous one', await js(`(() => { const fl = ML.app.doc.floating; const a = ML.app.L.nodes.get(fl[fl.length - 2].id), b = ML.app.L.nodes.get(fl[fl.length - 1].id); const box = ML.layout.subtreeBox(a); return b.y > box.maxY; })()`));
    await js('ML.app.run("undo"); ML.app.run("undo"); true');

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
    await js('ML.app.select(__t.byText("Create").id); ML.app.run("toggleCollapse"); true');
    const histBefore = await js('ML.app.history.idx');
    await js('ML.app.run("present"); true');
    await sleep(400);
    await key('Right'); await sleep(300);
    await key('Escape'); await sleep(400);
    check('presenting does not change the map (collapsed branches stay collapsed)', await js('__t.byText("Create").collapsed === true') && (await js('ML.app.history.idx')) === histBefore);
    await js('ML.app.select(__t.byText("Create").id); ML.app.run("toggleCollapse"); true');

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

    // ---- AI assistant: bring your own key, against a local mock of both API styles ----
    const http = require('node:http');
    const seen = [];
    const mock = http.createServer((req, res) => {
      let body = '';
      req.on('data', (c) => { body += c; });
      req.on('end', () => {
        const j = body ? JSON.parse(body) : null;
        const route = req.url.split('?')[0];
        const isAnthropic = !!req.headers['anthropic-version'];
        seen.push({ url: req.url, route, isAnthropic, auth: req.headers.authorization || req.headers['x-api-key'] || '', model: j && j.model, body: j });
        const send = (code, obj) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(obj)); };
        const bad = (req.headers.authorization || '') === 'Bearer wrong-key';
        if (bad) return send(401, { error: { message: 'Incorrect API key provided' } });
        if (route === '/v1/models' && isAnthropic) return send(200, { data: [{ type: 'model', id: 'claude-opus-5-5', display_name: 'Claude Opus 5.5', created_at: '2026-09-01T00:00:00Z' }], has_more: false, first_id: 'claude-opus-5-5', last_id: 'claude-opus-5-5' });
        if (route === '/v1/messages') return send(200, { id: 'msg_1', type: 'message', role: 'assistant', model: j.model, content: [{ type: 'text', text: 'Garden\n  Vegetables\n    Tomatoes\n  Flowers' }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 5, output_tokens: 5 } });
        if (route === '/v1/models') return send(200, { object: 'list', data: [{ id: 'mock-small', created: 100 }, { id: 'mock-large', created: 200 }, { id: 'text-embedding-mock', created: 300 }] });
        if (route === '/v1/chat/completions') {
          const prompt = j.messages[j.messages.length - 1].content;
          let out = '<think>planning…</think>\n```\nPodcast launch\n  - Planning\n    - Audience\n  - Equipment\n  - Marketing\n```';
          if (/NEW subtopics/.test(prompt)) out = 'Sponsorships\n  Local businesses\nListener survey';
          else if (/<text>/.test(prompt)) out = 'Coffee brewing\n  Methods\n    Pour over\n  Beans';
          return send(200, { id: 'c1', object: 'chat.completion', choices: [{ index: 0, finish_reason: 'stop', message: { role: 'assistant', content: out } }] });
        }
        send(404, { error: { message: 'not found ' + req.url } });
      });
    });
    await new Promise((r) => mock.listen(0, '127.0.0.1', r));
    const mockBase = `http://127.0.0.1:${mock.address().port}`;
    const waitFor = async (expr, ms) => { for (let i = 0; i < (ms || 6000) / 100; i++) { if (await js(expr)) return true; await sleep(100); } return false; };
    const dlg = (q) => `document.querySelector('#modal-root .modal ${q}')`;

    await js('ML.app.setDoc(ML.welcomeDoc()); true');
    await js('ML.app.run("ai"); true');
    check('AI dialog opens with a provider list', await waitFor(`!!${dlg('#ai-prov')} && ${dlg('#ai-prov')}.options.length >= 10`));
    // choose the custom OpenAI-compatible provider and point it at the mock
    await js(`(() => { const s = ${dlg('#ai-prov')}; s.value = 'custom'; s.dispatchEvent(new Event('change')); })(); true`);
    await waitFor(`!${dlg('#ai-base-row')}.hidden`);
    await js(`(() => { const b = ${dlg('#ai-base')}; b.value = '${mockBase}/v1'; b.dispatchEvent(new Event('change')); })(); true`);
    await waitFor(`/saved|models/.test(${dlg('#ai-status')}.textContent)`);
    await js(`(() => { ${dlg('#ai-key')}.value = 'test-key-123'; ${dlg('[data-key-save]')}.click(); })(); true`);
    check('a pasted key is saved and shown as saved', await waitFor(`/Key saved/.test(${dlg('#ai-key-row')}.textContent)`));
    await js(`${dlg('#ai-load')}.click(); true`);
    check('"Load models" lists the provider\'s chat models (newest first, embeddings hidden)', await waitFor(`[...${dlg('#ai-model-list')}.options].map((o) => o.value).join() === 'mock-large,mock-small'`), await js(`[...${dlg('#ai-model-list')}.options].map((o) => o.value).join()`));
    await shot('17-ai-dialog');
    await js(`(() => { const m = ${dlg('#ai-model')}; m.value = 'mock-large'; m.dispatchEvent(new Event('change')); ${dlg('#ai-topic')}.value = 'Launching a podcast'; ML.app.setDirty(false); ${dlg('[data-go]')}.click(); })(); true`);
    check('generate from a topic builds a new map', await waitFor('ML.app.doc.root.text === "Podcast launch" && ML.app.doc.root.children.length === 3 && ML.app.doc.root.children[0].children[0].text === "Audience"'), await js('ML.io.toText(ML.app.doc).slice(0, 120)'));
    const chat = seen.filter((x) => x.url === '/v1/chat/completions').pop();
    check('the request carries the key, the chosen model and the topic', chat && chat.auth === 'Bearer test-key-123' && chat.model === 'mock-large' && /Launching a podcast/.test(JSON.stringify(chat.body)));
    check('the map keeps the current theme and structure', (await js('ML.app.doc.settings.theme')) === 'fresh' && (await js('ML.app.dirty')) === true);

    // from pasted notes, added into the current map
    await js('ML.app.run("ai"); true');
    await waitFor(`!!${dlg('[data-ai-mode]')}`);
    await js(`(() => { ${dlg('[data-ai-mode] [data-val="text"]')}.click(); ${dlg('#ai-text')}.value = 'Coffee can be brewed many ways. Pour over is popular. Bean choice matters.'; ${dlg('#ai-into')}.checked = true; ML.app.select(ML.app.doc.root.id); ${dlg('[data-go]')}.click(); })(); true`);
    check('generate from notes can add a branch to the current map', await waitFor('ML.app.doc.root.text === "Podcast launch" && ML.app.doc.root.children.some((c) => c.text === "Coffee brewing" && c.children[0].children[0].text === "Pour over")'));

    // expand the selected topic
    await js('ML.app.select(ML.app.doc.root.children.find((c) => c.text === "Marketing").id); ML.app.run("ai", "expand"); true');
    await waitFor(`!!${dlg('[data-go]')} && ${dlg('[data-go]')}.textContent === 'Suggest subtopics'`);
    await js(`${dlg('[data-go]')}.click(); true`);
    check('expand adds AI subtopics under the selected topic', await waitFor('(() => { const m = ML.app.doc.root.children.find((c) => c.text === "Marketing"); return m.children.length === 2 && m.children[0].text === "Sponsorships" && m.children[0].children[0].text === "Local businesses"; })()'));
    const expandReq = seen.filter((x) => x.url === '/v1/chat/completions').pop();
    check('expand sends the topic path and existing subtopics as context', /Podcast launch > Marketing/.test(JSON.stringify(expandReq.body)));
    await js('ML.app.run("undo"); true');
    check('AI changes can be undone', await waitFor('ML.app.doc.root.children.find((c) => c.text === "Marketing").children.length === 0'));

    // a rejected key gives a clear message and keeps the dialog open
    await js('ML.app.run("ai"); true');
    await waitFor(`!!${dlg('[data-key-replace]')}`);
    await js(`${dlg('[data-key-replace]')}.click(); true`);
    await waitFor(`!!${dlg('#ai-key')}`);
    await js(`(() => { ${dlg('#ai-key')}.value = 'wrong-key'; ${dlg('#ai-topic')}.value = 'Anything'; ${dlg('[data-go]')}.click(); })(); true`);
    check('a rejected key shows a clear error', await waitFor(`/rejected the API key/.test(${dlg('#ai-status')}.textContent)`), await js(`${dlg('#ai-status')} && ${dlg('#ai-status')}.textContent`));
    check('the error keeps the dialog open to fix it', await js(`!!${dlg('[data-go]')} && !${dlg('[data-go]')}.disabled`));
    await key('Escape');

    // keys are stored encrypted, never in plain text
    const keysFile = path.join(app.getPath('userData'), 'ai-keys.json');
    const keysRaw = fs.existsSync(keysFile) ? fs.readFileSync(keysFile, 'utf8') : '';
    check('API keys are stored encrypted on disk', keysRaw.includes('custom') && !keysRaw.includes('wrong-key') && !keysRaw.includes('test-key-123'));

    // Anthropic through the official SDK (pointed at the mock only in self-test mode)
    const cfgFile = path.join(app.getPath('userData'), 'ai-config.json');
    const cfgNow = JSON.parse(fs.readFileSync(cfgFile, 'utf8'));
    cfgNow.baseUrls.anthropic = mockBase;
    fs.writeFileSync(cfgFile, JSON.stringify(cfgNow));
    await js('ML.platform.aiSetKey("anthropic", "sk-ant-test-0001").then(() => ML.platform.aiSetConfig({ provider: "anthropic" })).then(() => true)');
    await js('ML.app.setDirty(false); ML.app.run("ai"); true');
    await waitFor(`!!${dlg('#ai-prov')} && ${dlg('#ai-prov')}.value === 'anthropic'`);
    check('Anthropic models load through the SDK', await waitFor(`[...${dlg('#ai-model-list')}.options].some((o) => o.value === 'claude-opus-5-5')`), JSON.stringify(seen.filter((x) => x.isAnthropic).map((x) => x.url)) + ' status: ' + (await js(`${dlg('#ai-status')}.textContent`)));
    await js(`(() => { ${dlg('#ai-model')}.value = 'claude-opus-5-5'; ${dlg('#ai-topic')}.value = 'My garden'; ${dlg('[data-go]')}.click(); })(); true`);
    check('Anthropic generates a map', await waitFor('ML.app.doc.root.text === "Garden" && ML.app.doc.root.children[0].children[0].text === "Tomatoes"'));
    const msgReq = seen.filter((x) => x.isAnthropic && x.route === '/v1/messages').pop();
    check('the Anthropic request uses the chosen model, effort and refusal fallback', msgReq && msgReq.model === 'claude-opus-5-5' && /sk-ant-test-0001$/.test(msgReq.auth) && msgReq.body.fallbacks === 'default' && msgReq.body.output_config && msgReq.body.output_config.effort === 'medium',
      msgReq && JSON.stringify({ url: msgReq.url, auth: msgReq.auth, model: msgReq.model, fallbacks: msgReq.body.fallbacks, oc: msgReq.body.output_config }));
    await new Promise((r) => mock.close(r));

    // Ollama: a clear message when nothing is running
    await js('ML.platform.aiSetConfig({ provider: "ollama", for: "ollama", baseUrl: "http://127.0.0.1:9", model: "llama3" }).then(() => true)');
    const ollamaErr = await js('ML.platform.ai({ mode: "generate", topic: "x" }).then(() => "unexpected", (e) => e.message)');
    check('a local model server that is not running gives a clear message', /Make sure it is running/.test(ollamaErr), ollamaErr);
    await js('ML.platform.aiSetConfig({ provider: "custom" }).then(() => true)');

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
