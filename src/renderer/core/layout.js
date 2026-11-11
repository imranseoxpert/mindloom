/* Mindloom — measuring + layout algorithms + connector paths (pure; text measuring is injected) */
(function (ML) {
  'use strict';

  const r1 = (v) => Math.round(v * 10) / 10;

  // ---------- text wrapping ----------
  function wrapText(text, font, maxW, measure) {
    const out = [];
    const paras = String(text).split('\n');
    for (const para of paras) {
      if (!para.length) { out.push(''); continue; }
      const words = para.split(/(\s+)/);
      let line = '';
      for (const w of words) {
        const cand = line + w;
        if (measure(cand, font) <= maxW || !line.trim()) {
          // a single word longer than maxW gets broken by characters
          if (!line.trim() && measure(cand, font) > maxW && w.trim()) {
            let chunk = line;
            for (const ch of w) {
              if (measure(chunk + ch, font) > maxW && chunk) { out.push(chunk); chunk = ch; } else chunk += ch;
            }
            line = chunk;
          } else line = cand;
        } else {
          out.push(line.replace(/\s+$/, ''));
          line = w.replace(/^\s+/, '');
        }
      }
      out.push(line.replace(/\s+$/, ''));
    }
    return out.length ? out : [''];
  }

  // ---------- node measuring ----------
  function fontString(st) {
    return `${st.italic ? 'italic ' : ''}${st.bold ? 700 : 400} ${st.fontSize}px ${st.font}`;
  }

  function measureNode(node, depth, branch, ctx, isFloating) {
    const T = ML.themes;
    const st = T.resolveStyle(node, depth, branch, ctx.theme, ctx.settings, isFloating);
    const fs = st.fontSize;
    const font = fontString(st);
    const lh = Math.round(fs * 1.32);
    const maxW = (node.style && node.style.width) || (depth === 0 ? ctx.settings.maxWidth + 80 : ctx.settings.maxWidth);
    const text = node.text && node.text.length ? node.text : ' ';
    const lines = wrapText(text, font, maxW, ctx.measure);
    let textW = 0;
    for (const l of lines) textW = Math.max(textW, ctx.measure(l, font));
    textW = Math.max(textW, node.text ? 0 : fs * 3);
    const textH = lines.length * lh;

    const iconSize = Math.round(fs * 1.15);
    const lead = [];
    if (node.task) lead.push({ kind: 'task', val: node.task });
    for (const m of node.icons || []) lead.push({ kind: 'marker', val: m });
    const trail = [];
    if (node.link) trail.push({ kind: 'link', val: node.link });
    if (node.note) trail.push({ kind: 'note', val: node.note });
    if (node.collapsed && node.children.length && ctx.settings.layout === 'tree') { /* badge handled separately */ }
    const iconGap = 5;
    const leadW = lead.length ? lead.length * (iconSize + iconGap) : 0;
    const trailW = trail.length ? trail.length * (iconSize + iconGap) : 0;
    const rowW = leadW + textW + trailW;
    const rowH = Math.max(textH, lead.length || trail.length ? iconSize : 0);

    const tagFont = `600 11px ${st.font}`;
    const tags = (node.tags || []).map((t) => ({ text: t, w: Math.ceil(ctx.measure(t, tagFont)) + 14 }));
    const tagsW = tags.reduce((s, t) => s + t.w + 4, 0) - (tags.length ? 4 : 0);
    const tagsH = tags.length ? 18 : 0;

    let img = null;
    if (node.image && node.image.src) {
      const iw0 = node.image.w || 160, ih0 = node.image.h || 120;
      const iw = Math.min(iw0, maxW);
      img = { w: iw, h: (ih0 * iw) / iw0 };
    }
    const contentW = Math.max(rowW, tagsW, img ? img.w : 0);
    const contentH = (img ? img.h + 8 : 0) + rowH + (tags.length ? tagsH + 6 : 0);

    let padX = Math.round(fs * 0.85), padY = Math.round(fs * 0.5);
    if (depth === 0 && !isFloating) { padX = Math.round(fs * 1.0); padY = Math.round(fs * 0.62); }
    if (st.shape === 'underline') { padX = 6; padY = 5; }
    if (st.shape === 'none') { padX = 4; padY = 3; }
    let w = contentW + padX * 2, h = contentH + padY * 2;
    switch (st.shape) {
      case 'ellipse': w = w * 1.22 + 6; h = h * 1.32 + 4; break;
      case 'circle': { const d = Math.max(w, h) * 1.08 + 8; w = d; h = d; break; }
      case 'diamond': w = w * 1.5 + 8; h = h * 1.7 + 6; break;
      case 'hexagon': w = w + h * 0.7; break;
      case 'parallelogram': w = w + h * 0.6; break;
      case 'cloud': w = w * 1.25 + 14; h = h * 1.45 + 10; break;
      case 'pill': w = w + h * 0.35; break;
      default: break;
    }
    w = Math.ceil(Math.max(w, depth === 0 ? 80 : 36));
    h = Math.ceil(h);
    const cx = (w - contentW) / 2, cy = (h - contentH) / 2;
    const rowY = cy + (img ? img.h + 8 : 0);
    return {
      w, h, style: st, font, lines, lh, fs, iconSize, iconGap, lead, trail, tags, tagsH, img,
      content: { x: cx, y: cy, w: contentW, h: contentH },
      row: { x: cx + (contentW - rowW) / 2, y: rowY, w: rowW, h: rowH, textX: leadW, textW, textH },
      tagsY: rowY + rowH + 6, tagsX: cx + (contentW - tagsW) / 2,
      imgPos: img ? { x: cx + (contentW - img.w) / 2, y: cy } : null,
    };
  }

  // ---------- visual tree ----------
  function buildVT(node, depth, branch, ctx, isFloating, parent) {
    const box = measureNode(node, depth, branch, ctx, isFloating);
    const t = { id: node.id, node, depth, branch, box, w: box.w, h: box.h, x: 0, y: 0, parent, floating: isFloating, children: [] };
    if (!node.collapsed) {
      node.children.forEach((c, i) => {
        const b = depth === 0 ? (isFloating ? branch : i) : branch;
        t.children.push(buildVT(c, depth + 1, b, ctx, isFloating, t));
      });
    }
    return t;
  }

  // ---------- horizontal logic tree ----------
  function sizeH(t, s) {
    const gap = t.depth === 0 ? s.vGap * 1.8 : s.vGap;
    if (!t.children.length) return (t.sh = t.h);
    let sum = 0;
    for (const c of t.children) sum += sizeH(c, s);
    sum += gap * (t.children.length - 1);
    return (t.sh = Math.max(t.h, sum));
  }
  function placeH(t, x, cy, dir, s) {
    t.x = dir > 0 ? x : x - t.w;
    t.y = cy - t.h / 2;
    t.dir = dir > 0 ? 'r' : 'l';
    if (!t.children.length) return;
    const gap = t.depth === 0 ? s.vGap * 1.8 : s.vGap;
    const hg = t.depth === 0 ? s.hGap * 1.25 : s.hGap;
    const childX = dir > 0 ? t.x + t.w + hg : t.x - hg;
    let total = -gap;
    for (const c of t.children) total += c.sh + gap;
    let y = cy - total / 2;
    for (const c of t.children) {
      placeH(c, childX, y + c.sh / 2, dir, s);
      y += c.sh + gap;
    }
  }

  // ---------- vertical org chart ----------
  // Inner levels spread horizontally; a group of leaves is stacked under its parent so wide maps stay readable.
  const stackIndent = (s) => Math.max(22, Math.min(40, s.hGap * 0.5));
  const stackGap = (s) => Math.max(6, s.vGap * 0.6);
  function isLeafGroup(t) { return t.depth >= 1 && t.children.length > 0 && t.children.every((c) => !c.children.length); }
  function sizeV(t, s) {
    const gap = Math.max(14, s.vGap + 8);
    if (!t.children.length) return (t.sw = t.w);
    if (isLeafGroup(t)) {
      let m = t.w;
      for (const c of t.children) m = Math.max(m, stackIndent(s) + c.w);
      return (t.sw = m);
    }
    let sum = 0;
    for (const c of t.children) sum += sizeV(c, s);
    sum += gap * (t.children.length - 1);
    return (t.sw = Math.max(t.w, sum));
  }
  function placeV(t, cx, y, dir, s) {
    if (isLeafGroup(t)) {
      const left = cx - t.sw / 2, g = stackGap(s);
      t.x = left;
      t.y = dir > 0 ? y : y - t.h;
      t.dir = dir > 0 ? 'tree' : 'tree-up';
      let cy = dir > 0 ? t.y + t.h + g : t.y - g;
      for (const c of t.children) {
        c.x = left + stackIndent(s);
        c.y = dir > 0 ? cy : cy - c.h;
        c.dir = t.dir;
        cy = dir > 0 ? c.y + c.h + g : c.y - g;
      }
      return;
    }
    t.x = cx - t.w / 2;
    t.y = dir > 0 ? y : y - t.h;
    t.dir = dir > 0 ? 'd' : 'u';
    if (!t.children.length) return;
    const gap = Math.max(14, s.vGap + 8);
    const lg = Math.max(36, s.hGap * 0.85);
    const childY = dir > 0 ? t.y + t.h + lg : t.y - lg;
    let total = -gap;
    for (const c of t.children) total += c.sw + gap;
    let x = cx - total / 2;
    for (const c of t.children) {
      placeV(c, x + c.sw / 2, childY, dir, s);
      x += c.sw + gap;
    }
  }

  // ---------- indented tree ----------
  function sizeT(t, s) {
    const gap = Math.max(8, s.vGap * 0.7);
    let hsum = t.h;
    for (const c of t.children) hsum += gap + sizeT(c, s);
    return (t.th = hsum);
  }
  function placeT(t, x, y, s) {
    t.x = x; t.y = y; t.dir = 'tree';
    const gap = Math.max(8, s.vGap * 0.7);
    const indent = Math.max(26, Math.min(48, s.hGap * 0.65));
    let cy = y + t.h + gap;
    for (const c of t.children) {
      placeT(c, x + indent, cy, s);
      cy += c.th + gap;
    }
  }
  function widthT(t) {
    let m = t.x + t.w;
    for (const c of t.children) m = Math.max(m, widthT(c));
    return m;
  }

  // ---------- radial ----------
  function leaves(t) { return t.children.length ? t.children.reduce((s, c) => s + leaves(c), 0) : 1; }
  function placeRadial(root, s) {
    root.x = -root.w / 2; root.y = -root.h / 2; root.dir = 'radial';
    const byDepth = [];
    const assign = (t, a0, a1) => {
      t.a = (a0 + a1) / 2; t.span = a1 - a0;
      (byDepth[t.depth] = byDepth[t.depth] || []).push(t);
      const total = leaves(t);
      let a = a0;
      for (const c of t.children) {
        const span = ((a1 - a0) * leaves(c)) / total;
        assign(c, a, a + span);
        a += span;
      }
    };
    assign(root, -Math.PI / 2, (Math.PI * 3) / 2);
    const radius = [0];
    const minGap = [0];
    for (let d = 1; d < byDepth.length; d++) {
      const prevHalf = Math.max(...byDepth[d - 1].map((t) => Math.hypot(t.w, t.h) / 2));
      const half = Math.max(...byDepth[d].map((t) => Math.hypot(t.w, t.h) / 2));
      minGap[d] = prevHalf + half + s.hGap * 0.6;
      let r = radius[d - 1] + minGap[d];
      for (const t of byDepth[d]) {
        // room a box needs along the ring: wide labels matter at the top/bottom, tall ones at the sides
        const along = Math.abs(t.w * Math.sin(t.a)) + Math.abs(t.h * Math.cos(t.a));
        const need = ((along + s.vGap) * 1.12) / Math.max(0.05, Math.min(t.span, Math.PI));
        r = Math.max(r, need);
      }
      radius[d] = r;
    }
    // keep rings evenly spread instead of crowding the centre when an outer ring had to grow
    for (let d = byDepth.length - 2; d >= 1; d--) {
      const even = radius[d + 1] * (d / (d + 1)) * 0.8;
      radius[d] = Math.max(radius[d], Math.min(even, radius[d + 1] - minGap[d + 1]));
    }
    for (let d = 1; d < byDepth.length; d++) {
      for (const t of byDepth[d]) {
        const cx = Math.cos(t.a) * radius[d], cy = Math.sin(t.a) * radius[d];
        t.x = cx - t.w / 2; t.y = cy - t.h / 2; t.dir = 'radial';
      }
    }
  }

  // ---------- timeline ----------
  function placeTimeline(root, s, extras) {
    root.x = 0; root.y = -root.h / 2; root.dir = 'r';
    let x = root.x + root.w + s.hGap;
    const gap = Math.max(8, s.vGap * 0.7);
    root.children.forEach((c, i) => {
      c.x = x; c.y = -c.h / 2; c.dir = i % 2 === 0 ? 'tree' : 'tree-up';
      let colW = c.w;
      if (c.children.length) {
        let total = 0;
        for (const g of c.children) total += sizeT(g, s) + gap;
        if (i % 2 === 0) {
          let y = c.y + c.h + gap + 6;
          for (const g of c.children) { placeT(g, x + 22, y, s); y += g.th + gap; colW = Math.max(colW, widthT(g) - x); }
        } else {
          let y = c.y - 6 - total;
          for (const g of c.children) { placeT(g, x + 22, y, s); y += g.th + gap; colW = Math.max(colW, widthT(g) - x); }
        }
      }
      x += colW + s.hGap * 0.8;
    });
    extras.push({ type: 'axis', x1: root.x + root.w, x2: x - s.hGap * 0.4, y: 0 });
  }

  // ---------- fishbone ----------
  function flattenRows(t, depth, out) {
    for (const c of t.children) { out.push({ t: c, d: depth }); flattenRows(c, depth + 1, out); }
    return out;
  }
  function placeFishbone(root, s, extras) {
    root.x = 0; root.y = -root.h / 2; root.dir = 'l';
    let sx = root.x - 40;
    const slope = 0.55;
    const kids = root.children;
    let pairW = 0;
    kids.forEach((c, i) => {
      const above = i % 2 === 0;
      const rows = flattenRows(c, 0, []);
      const rowGap = Math.max(6, s.vGap * 0.6);
      let rowsH = 0, maxRowW = 0;
      for (const r of rows) { rowsH += r.t.h + rowGap; maxRowW = Math.max(maxRowW, r.t.w + r.d * 22); }
      const L = Math.max(70, rowsH + 24);
      const topX = sx - slope * L;
      c.dir = above ? 'fish-up' : 'fish-down';
      c.x = topX - c.w / 2;
      c.y = above ? -L - c.h : L;
      c.bone = { x1: sx, y1: 0, x2: topX, y2: above ? -L : L };
      // rows hug the bone, nearest to the topic first
      let y = above ? -L + 12 : 12;
      for (const r of rows) {
        const cy = y + r.t.h / 2;
        const boneX = sx - slope * Math.abs(cy);
        r.t.x = boneX - 10 - r.d * 22 - r.t.w;
        r.t.y = y;
        r.t.dir = 'fish-row';
        r.t.boneX = boneX;
        y += r.t.h + rowGap;
      }
      const slotW = Math.max(c.w / 2 + slope * L, slope * L + maxRowW + 20) + 30;
      pairW = Math.max(pairW, slotW);
      if (!above || i === kids.length - 1) { sx -= pairW; pairW = 0; }
    });
    extras.push({ type: 'spine', x1: root.x, x2: Math.min(sx + 20, root.x - 60), y: 0 });
  }

  // ---------- main entry ----------
  /**
   * ctx = { theme, settings, measure(text, font) -> px, focusId? }
   * returns { nodes: Map(id -> t), list: [t], edges, boundaries, extras, bounds }
   */
  function layout(doc, ctx) {
    const s = ctx.settings;
    const M = ML.model;
    let mainRoot = doc.root;
    if (ctx.focusId) { const f = M.getNode(doc, ctx.focusId); if (f) mainRoot = f; }
    const extras = [];
    const root = buildVT(mainRoot, 0, -1, ctx, false, null);
    const lay = s.layout;
    if (lay === 'mindmap') {
      root.x = -root.w / 2; root.y = -root.h / 2; root.dir = 'both';
      root.children.forEach((c) => sizeH(c, s));
      const total = root.children.reduce((a, c) => a + c.sh, 0);
      const right = [], left = [];
      let acc = 0;
      root.children.forEach((c, i) => {
        const side = c.node.side;
        if (side === 'left') left.push(c);
        else if (side === 'right') right.push(c);
        else if (root.children.length === 1 || acc < total / 2 || i === 0) { right.push(c); acc += c.sh; }
        else left.push(c);
      });
      const gap = s.vGap * 1.8, hg = s.hGap * 1.25;
      const stack = (arr, dir) => {
        let tot = -gap;
        for (const c of arr) tot += c.sh + gap;
        let y = -tot / 2;
        for (const c of arr) {
          placeH(c, dir > 0 ? root.x + root.w + hg : root.x - hg, y + c.sh / 2, dir, s);
          c.side = dir > 0 ? 'right' : 'left';
          y += c.sh + gap;
        }
      };
      stack(right, 1);
      stack(left.reverse(), -1);
    } else if (lay === 'right' || lay === 'left') {
      sizeH(root, s);
      placeH(root, lay === 'right' ? 0 : 0, 0, lay === 'right' ? 1 : -1, s);
    } else if (lay === 'down' || lay === 'up') {
      sizeV(root, s);
      placeV(root, 0, 0, lay === 'down' ? 1 : -1, s);
    } else if (lay === 'tree') {
      sizeT(root, s);
      placeT(root, 0, 0, s);
    } else if (lay === 'radial') {
      placeRadial(root, s);
    } else if (lay === 'timeline') {
      placeTimeline(root, s, extras);
    } else if (lay === 'fishbone') {
      placeFishbone(root, s, extras);
    } else {
      sizeH(root, s); placeH(root, 0, 0, 1, s);
    }

    const forest = [root];
    if (!ctx.focusId) {
      const offset = (doc.root.children.length || 1);
      doc.floating.forEach((f, i) => {
        const ft = buildVT(f, 0, offset + i, ctx, true, null);
        sizeH(ft, s);
        placeH(ft, f.x || 0, (f.y || 0) + ft.h / 2, 1, s);
        forest.push(ft);
      });
    }

    const nodes = new Map();
    const list = [];
    const edges = [];
    const visit = (t) => {
      nodes.set(t.id, t);
      list.push(t);
      for (const c of t.children) {
        edges.push(makeEdge(t, c, s));
        visit(c);
      }
    };
    forest.forEach(visit);

    const boundaries = [];
    for (const t of list) {
      if (!t.node.boundary) continue;
      const b = subtreeBox(t);
      const pad = 12;
      boundaries.push({
        id: t.id, x: b.minX - pad, y: b.minY - pad - (t.node.boundaryLabel ? 22 : 0),
        w: b.maxX - b.minX + pad * 2, h: b.maxY - b.minY + pad * 2 + (t.node.boundaryLabel ? 22 : 0),
        label: t.node.boundaryLabel || '', color: t.box.style.line, depth: t.depth,
      });
    }

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    const grow = (x, y, w, h) => { minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x + w); maxY = Math.max(maxY, y + h); };
    for (const t of list) grow(t.x, t.y, t.w + (t.node.collapsed && t.node.children.length ? 26 : 0), t.h + 4);
    for (const b of boundaries) grow(b.x, b.y, b.w, b.h);
    for (const e of extras) {
      if (e.type === 'spine' || e.type === 'axis') grow(Math.min(e.x1, e.x2), e.y - 2, Math.abs(e.x2 - e.x1), 4);
    }
    if (!list.length) { minX = minY = 0; maxX = maxY = 1; }
    return { root, forest, nodes, list, edges, boundaries, extras, bounds: { minX, minY, maxX, maxY, w: maxX - minX, h: maxY - minY } };
  }

  function subtreeBox(t) {
    let minX = t.x, minY = t.y, maxX = t.x + t.w, maxY = t.y + t.h;
    const rec = (n) => {
      for (const c of n.children) {
        minX = Math.min(minX, c.x); minY = Math.min(minY, c.y);
        maxX = Math.max(maxX, c.x + c.w); maxY = Math.max(maxY, c.y + c.h);
        rec(c);
      }
    };
    rec(t);
    return { minX, minY, maxX, maxY };
  }

  // ---------- connectors ----------
  function anchorBottom(t) { return t.box.style.shape === 'underline'; }
  function clipToBox(t, px, py) {
    const cx = t.x + t.w / 2, cy = t.y + t.h / 2;
    const dx = px - cx, dy = py - cy;
    if (!dx && !dy) return { x: cx, y: cy };
    const sx = (t.w / 2) / Math.abs(dx || 1e-9), sy = (t.h / 2) / Math.abs(dy || 1e-9);
    const k = Math.min(sx, sy, 1);
    return { x: cx + dx * k, y: cy + dy * k };
  }

  function makeEdge(p, c, s) {
    const st = c.box.style;
    const width = s.lineWidth * (c.depth === 1 ? 1.4 : 1);
    const e = { from: p.id, to: c.id, color: st.line, width, depth: c.depth, orient: 'h' };
    const pcy = p.y + p.h / 2, ccy = c.y + c.h / 2;
    const dir = p.dir === 'both' ? (c.side === 'left' ? 'l' : 'r') : p.dir;
    if (dir === 'r' || dir === 'l') {
      const right = dir === 'r';
      e.p1 = { x: right ? p.x + p.w : p.x, y: anchorBottom(p) && p.depth > 0 ? p.y + p.h : pcy };
      e.p2 = { x: right ? c.x : c.x + c.w, y: anchorBottom(c) ? c.y + c.h : ccy };
      if (p.depth === 0 && p.dir === 'both') e.p1.x = right ? p.x + p.w - Math.min(10, p.w / 4) : p.x + Math.min(10, p.w / 4);
    } else if (dir === 'd' || dir === 'u') {
      e.orient = 'v';
      e.p1 = { x: p.x + p.w / 2, y: dir === 'd' ? p.y + p.h : p.y };
      e.p2 = { x: c.x + c.w / 2, y: dir === 'd' ? c.y : c.y + c.h };
    } else if (dir === 'tree' || dir === 'tree-up') {
      e.orient = 'tree';
      e.p1 = { x: p.x + Math.min(18, p.w / 3), y: dir === 'tree' ? p.y + p.h : p.y };
      e.p2 = { x: c.x, y: anchorBottom(c) ? c.y + c.h : ccy };
    } else if (dir === 'fish-up' || dir === 'fish-down') {
      // rows of a fishbone attach to the bone itself
      e.orient = 'h';
      e.p1 = { x: c.boneX, y: anchorBottom(c) ? c.y + c.h : ccy };
      e.p2 = { x: c.x + c.w, y: e.p1.y };
      if (c.dir === 'fish-row' && c.parent && c.parent.dir === 'fish-row') {
        e.orient = 'tree';
        e.p1 = { x: p.x + p.w - 12, y: p.y + p.h };
        e.p2 = { x: c.x + c.w, y: anchorBottom(c) ? c.y + c.h : ccy };
      }
    } else if (dir === 'fish-row') {
      e.orient = 'tree';
      e.p1 = { x: p.x + p.w - 12, y: p.y + p.h };
      e.p2 = { x: c.x + c.w, y: anchorBottom(c) ? c.y + c.h : ccy };
    } else {
      e.orient = 'free';
      e.p1 = clipToBox(p, c.x + c.w / 2, ccy);
      e.p2 = clipToBox(c, p.x + p.w / 2, pcy);
    }
    // fishbone main bones + timeline axis children are drawn as straight segments
    if (c.bone) { e.orient = 'bone'; e.p1 = { x: c.bone.x1, y: c.bone.y1 }; e.p2 = { x: c.bone.x2, y: c.bone.y2 }; }
    if (p.depth === 0 && s.layout === 'timeline' && !p.floating) {
      e.orient = 'axisdot'; e.p1 = { x: c.x - 6, y: 0 }; e.p2 = { x: c.x, y: 0 };
    }
    return e;
  }

  function f(n) { return r1(n); }
  function corner(x1, y1, x2, y2, rx) { return `Q ${f(x1)} ${f(y1)} ${f(x2)} ${f(y2)}`; }

  /** Build an SVG path for an edge. Returns {d, filled}. */
  function edgePath(e, style) {
    const { x: x1, y: y1 } = e.p1, { x: x2, y: y2 } = e.p2;
    const dx = x2 - x1, dy = y2 - y1;
    if (e.orient === 'bone' || e.orient === 'axisdot') return { d: `M ${f(x1)} ${f(y1)} L ${f(x2)} ${f(y2)}` };
    let orient = e.orient;
    if (orient === 'free') {
      if (style === 'straight' || style === 'elbow' || style === 'rounded') return { d: `M ${f(x1)} ${f(y1)} L ${f(x2)} ${f(y2)}` };
      orient = Math.abs(dx) >= Math.abs(dy) ? 'h' : 'v';
    }
    if (orient === 'tree') {
      if (style === 'straight') return { d: `M ${f(x1)} ${f(y1)} L ${f(x2)} ${f(y2)}` };
      if (style === 'curve' || style === 'tapered') return { d: `M ${f(x1)} ${f(y1)} Q ${f(x1)} ${f(y2)} ${f(x2)} ${f(y2)}` };
      if (style === 'elbow') return { d: `M ${f(x1)} ${f(y1)} V ${f(y2)} H ${f(x2)}` };
      const r = Math.min(10, Math.abs(dy), Math.abs(dx));
      const sy = Math.sign(dy) || 1, sx = Math.sign(dx) || 1;
      return { d: `M ${f(x1)} ${f(y1)} V ${f(y2 - sy * r)} ${corner(x1, y2, x1 + sx * r, y2)} H ${f(x2)}` };
    }
    const H = orient === 'h';
    // generic helpers in "main axis" space: a = along growth direction, b = across
    const P = (a, b) => (H ? `${f(a)} ${f(b)}` : `${f(b)} ${f(a)}`);
    const a1 = H ? x1 : y1, b1 = H ? y1 : x1, a2 = H ? x2 : y2, b2 = H ? y2 : x2;
    const da = a2 - a1;
    switch (style) {
      case 'straight': return { d: `M ${P(a1, b1)} L ${P(a2, b2)}` };
      case 'elbow': {
        const am = a1 + da / 2;
        return { d: `M ${P(a1, b1)} L ${P(am, b1)} L ${P(am, b2)} L ${P(a2, b2)}` };
      }
      case 'rounded': {
        const am = a1 + da / 2;
        const r = Math.min(10, Math.abs(b2 - b1) / 2, Math.abs(da) / 2);
        if (r < 0.5) return { d: `M ${P(a1, b1)} L ${P(a2, b2)}` };
        const sa = Math.sign(da) || 1, sb = Math.sign(b2 - b1) || 1;
        return {
          d: `M ${P(a1, b1)} L ${P(am - sa * r, b1)} Q ${P(am, b1)} ${P(am, b1 + sb * r)} L ${P(am, b2 - sb * r)} Q ${P(am, b2)} ${P(am + sa * r, b2)} L ${P(a2, b2)}`,
        };
      }
      case 'tapered': {
        const w1 = Math.max(1.5, e.width * (e.depth === 1 ? 2.6 : 1.7)), w2 = Math.max(0.6, e.width * 0.35);
        const c1 = a1 + da * 0.5, c2 = a2 - da * 0.5;
        return {
          filled: true,
          d: `M ${P(a1, b1 - w1 / 2)} C ${P(c1, b1 - w1 / 2)} ${P(c2, b2 - w2 / 2)} ${P(a2, b2 - w2 / 2)} L ${P(a2, b2 + w2 / 2)} C ${P(c2, b2 + w2 / 2)} ${P(c1, b1 + w1 / 2)} ${P(a1, b1 + w1 / 2)} Z`,
        };
      }
      default: {
        const c1 = a1 + da * 0.5, c2 = a2 - da * 0.5;
        return { d: `M ${P(a1, b1)} C ${P(c1, b1)} ${P(c2, b2)} ${P(a2, b2)}` };
      }
    }
  }

  /** Relationship curve between two laid-out nodes. */
  function relationPath(a, b, rel) {
    const ac = { x: a.x + a.w / 2, y: a.y + a.h / 2 }, bc = { x: b.x + b.w / 2, y: b.y + b.h / 2 };
    const dx = bc.x - ac.x, dy = bc.y - ac.y, len = Math.hypot(dx, dy) || 1;
    const nx = -dy / len, ny = dx / len;
    const bend = rel.bend != null ? rel.bend : Math.min(120, len * 0.35);
    const c1 = { x: ac.x + dx * 0.25 + nx * bend, y: ac.y + dy * 0.25 + ny * bend };
    const c2 = { x: ac.x + dx * 0.75 + nx * bend, y: ac.y + dy * 0.75 + ny * bend };
    const p1 = clipToBox(a, c1.x, c1.y), p2 = clipToBox(b, c2.x, c2.y);
    const mid = { x: 0.125 * p1.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * p2.x, y: 0.125 * p1.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * p2.y };
    const ang = Math.atan2(p2.y - c2.y, p2.x - c2.x);
    return { d: `M ${f(p1.x)} ${f(p1.y)} C ${f(c1.x)} ${f(c1.y)} ${f(c2.x)} ${f(c2.y)} ${f(p2.x)} ${f(p2.y)}`, p1, p2, mid, ang };
  }

  ML.layout = { wrapText, measureNode, layout, edgePath, relationPath, fontString, subtreeBox };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
