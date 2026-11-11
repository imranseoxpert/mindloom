/* Mindloom — SVG string renderer (shared by the live canvas and SVG/PNG/PDF export) */
(function (ML) {
  'use strict';

  const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  const n = (v) => Math.round(v * 10) / 10;

  // ---------- background patterns (one tile definition, used for CSS + export) ----------
  function patternTile(id, ink) {
    switch (id) {
      case 'dots': return { size: 22, body: `<circle cx="11" cy="11" r="1.4" fill="${ink}"/>` };
      case 'grid': return { size: 24, body: `<path d="M24 0H0V24" fill="none" stroke="${ink}" stroke-width="1"/>` };
      case 'graph': return {
        size: 100,
        body: `<path d="M20 0V100M40 0V100M60 0V100M80 0V100M0 20H100M0 40H100M0 60H100M0 80H100" stroke="${ink}" stroke-opacity="0.55" stroke-width="0.6" fill="none"/><path d="M100 0H0V100" fill="none" stroke="${ink}" stroke-width="1.4"/>`,
      };
      case 'lines': return { size: 30, body: `<path d="M0 29.5H30" stroke="${ink}" stroke-width="1"/>` };
      case 'cross': return { size: 28, body: `<path d="M14 9V19M9 14H19" stroke="${ink}" stroke-width="1.3" stroke-linecap="round"/>` };
      case 'diagonal': return { size: 18, body: `<path d="M-2 2L2 -2M0 18L18 0M16 20L20 16" stroke="${ink}" stroke-width="1"/>` };
      case 'isometric': return {
        size: 24, h: 42,
        body: `<g fill="${ink}"><circle cx="0" cy="0" r="1.3"/><circle cx="24" cy="0" r="1.3"/><circle cx="12" cy="21" r="1.3"/><circle cx="0" cy="42" r="1.3"/><circle cx="24" cy="42" r="1.3"/></g>`,
      };
      default: return null;
    }
  }
  function patternCSS(id, ink, zoom, panX, panY) {
    const t = patternTile(id, ink);
    if (!t) return { image: 'none', size: 'auto', position: '0 0' };
    const h = t.h || t.size;
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${t.size}" height="${h}">${t.body}</svg>`;
    return {
      image: `url("data:image/svg+xml;utf8,${encodeURIComponent(svg)}")`,
      size: `${t.size * zoom}px ${h * zoom}px`,
      position: `${panX}px ${panY}px`,
    };
  }

  // ---------- shapes ----------
  function cloudPath(w, h) {
    const cx = w / 2, cy = h / 2, rx = w / 2 - 6, ry = h / 2 - 6, N = 11;
    const pts = [];
    for (let i = 0; i < N; i++) {
      const a = (i / N) * Math.PI * 2 - Math.PI / 2;
      pts.push([cx + Math.cos(a) * rx, cy + Math.sin(a) * ry]);
    }
    let d = `M ${n(pts[0][0])} ${n(pts[0][1])}`;
    for (let i = 1; i <= N; i++) {
      const p = pts[i % N], q = pts[i - 1];
      const r = Math.hypot(p[0] - q[0], p[1] - q[1]) * 0.62;
      d += ` A ${n(r)} ${n(r)} 0 0 1 ${n(p[0])} ${n(p[1])}`;
    }
    return d + ' Z';
  }

  function shapeSVG(t, isRoot) {
    const st = t.box.style, w = t.w, h = t.h;
    const fill = st.fill || 'transparent';
    const stroke = st.border || 'none';
    const sw = st.borderWidth || 0;
    const dash = st.borderDash === 'dashed' ? ' stroke-dasharray="6 4"' : st.borderDash === 'dotted' ? ' stroke-dasharray="1.5 3" stroke-linecap="round"' : '';
    const common = `fill="${fill}" stroke="${sw ? stroke : 'none'}" stroke-width="${sw}"${dash} class="shape"`;
    switch (st.shape) {
      case 'rect': return `<rect width="${w}" height="${h}" rx="2" ${common}/>`;
      case 'pill': return `<rect width="${w}" height="${h}" rx="${n(h / 2)}" ${common}/>`;
      case 'ellipse': return `<ellipse cx="${n(w / 2)}" cy="${n(h / 2)}" rx="${n(w / 2)}" ry="${n(h / 2)}" ${common}/>`;
      case 'circle': return `<circle cx="${n(w / 2)}" cy="${n(h / 2)}" r="${n(w / 2)}" ${common}/>`;
      case 'diamond': return `<polygon points="${n(w / 2)},0 ${w},${n(h / 2)} ${n(w / 2)},${h} 0,${n(h / 2)}" ${common}/>`;
      case 'hexagon': { const k = h * 0.35; return `<polygon points="${n(k)},0 ${n(w - k)},0 ${w},${n(h / 2)} ${n(w - k)},${h} ${n(k)},${h} 0,${n(h / 2)}" ${common}/>`; }
      case 'parallelogram': { const k = h * 0.3; return `<polygon points="${n(k)},0 ${w},0 ${n(w - k)},${h} 0,${h}" ${common}/>`; }
      case 'cloud': return `<path d="${cloudPath(w, h)}" ${common}/>`;
      case 'underline': {
        const bg = t.node.style && t.node.style.fill ? t.node.style.fill : 'transparent';
        const lw = Math.max(1.5, (st.borderWidth || 1.2) + 0.8);
        return `<rect width="${w}" height="${h}" rx="4" fill="${bg}" class="shape"/><path d="M0 ${n(h)} H${w}" stroke="${t.node.style && t.node.style.border ? st.border : st.line}" stroke-width="${lw}" stroke-linecap="round"/>`;
      }
      case 'none': {
        const bg = t.node.style && t.node.style.fill ? t.node.style.fill : 'transparent';
        return `<rect width="${w}" height="${h}" rx="4" fill="${bg}" class="shape"/>`;
      }
      default: return `<rect width="${w}" height="${h}" rx="${isRoot ? 12 : n(Math.min(8, h / 3))}" ${common}/>`;
    }
  }

  // ---------- markers ----------
  function star(cx, cy, r) {
    const pts = [];
    for (let i = 0; i < 10; i++) {
      const rr = i % 2 ? r * 0.45 : r;
      const a = -Math.PI / 2 + (i * Math.PI) / 5;
      pts.push(`${n(cx + Math.cos(a) * rr)},${n(cy + Math.sin(a) * rr)}`);
    }
    return pts.join(' ');
  }
  function markerSVG(kind, val, x, y, s, color) {
    const T = ML.themes;
    const cx = x + s / 2, cy = y + s / 2, r = s / 2;
    if (kind === 'task') {
      if (val === 'done') return `<g data-act="task" class="icon"><rect x="${n(x + 1)}" y="${n(y + 1)}" width="${n(s - 2)}" height="${n(s - 2)}" rx="4" fill="#2f9e44"/><path d="M${n(x + s * 0.27)} ${n(cy)} l${n(s * 0.16)} ${n(s * 0.17)} l${n(s * 0.32)} ${n(-s * 0.34)}" stroke="#fff" stroke-width="${n(s * 0.12)}" fill="none" stroke-linecap="round" stroke-linejoin="round"/></g>`;
      return `<g data-act="task" class="icon"><rect x="${n(x + 1.5)}" y="${n(y + 1.5)}" width="${n(s - 3)}" height="${n(s - 3)}" rx="4" fill="#fff" stroke="${color}" stroke-width="1.6"/></g>`;
    }
    if (kind === 'link') return `<g data-act="link" class="icon"><title>${esc(val)}</title><rect x="${x}" y="${y}" width="${s}" height="${s}" fill="transparent"/><path d="M${n(x + s * 0.42)} ${n(y + s * 0.58)} l${n(s * 0.16)} ${n(-s * 0.16)} M${n(x + s * 0.5)} ${n(y + s * 0.3)} l${n(s * 0.08)} ${n(-s * 0.08)} a${n(s * 0.16)} ${n(s * 0.16)} 0 0 1 ${n(s * 0.23)} ${n(s * 0.23)} l${n(-s * 0.16)} ${n(s * 0.16)} a${n(s * 0.16)} ${n(s * 0.16)} 0 0 1 ${n(-s * 0.23)} 0 M${n(x + s * 0.5)} ${n(y + s * 0.7)} l${n(-s * 0.08)} ${n(s * 0.08)} a${n(s * 0.16)} ${n(s * 0.16)} 0 0 1 ${n(-s * 0.23)} ${n(-s * 0.23)} l${n(s * 0.16)} ${n(-s * 0.16)} a${n(s * 0.16)} ${n(s * 0.16)} 0 0 1 ${n(s * 0.23)} 0" stroke="#1c7ed6" stroke-width="${n(s * 0.1)}" fill="none" stroke-linecap="round"/></g>`;
    if (kind === 'note') return `<g data-act="note" class="icon"><title>${esc(val.slice(0, 300))}</title><rect x="${n(x + s * 0.15)}" y="${n(y + s * 0.08)}" width="${n(s * 0.7)}" height="${n(s * 0.84)}" rx="2" fill="#fff3bf" stroke="#f59f00" stroke-width="1.2"/><path d="M${n(x + s * 0.3)} ${n(y + s * 0.35)}h${n(s * 0.4)}M${n(x + s * 0.3)} ${n(y + s * 0.52)}h${n(s * 0.4)}M${n(x + s * 0.3)} ${n(y + s * 0.69)}h${n(s * 0.25)}" stroke="#e67700" stroke-width="1.2"/></g>`;
    const m = val;
    let mm;
    if ((mm = /^p(\d)$/.exec(m))) {
      const c = T.PRIORITY_COLORS[(+mm[1] - 1) % 6];
      return `<g class="icon" data-act="marker" data-marker="${m}"><circle cx="${n(cx)}" cy="${n(cy)}" r="${n(r)}" fill="${c}"/><text x="${n(cx)}" y="${n(cy + 0.5)}" text-anchor="middle" dominant-baseline="central" font-size="${n(s * 0.62)}" font-weight="700" fill="#fff" font-family="-apple-system, 'Segoe UI', Arial, sans-serif">${mm[1]}</text></g>`;
    }
    if ((mm = /^pr(\d+)$/.exec(m))) {
      const frac = Math.min(100, +mm[1]) / 100;
      let pie = '';
      if (frac >= 1) pie = `<circle cx="${n(cx)}" cy="${n(cy)}" r="${n(r - 1)}" fill="#2f9e44"/><path d="M${n(x + s * 0.28)} ${n(cy)} l${n(s * 0.15)} ${n(s * 0.16)} l${n(s * 0.3)} ${n(-s * 0.32)}" stroke="#fff" stroke-width="${n(s * 0.11)}" fill="none" stroke-linecap="round"/>`;
      else if (frac > 0) {
        const a = frac * Math.PI * 2 - Math.PI / 2;
        const ex = cx + Math.cos(a) * (r - 1), ey = cy + Math.sin(a) * (r - 1);
        pie = `<path d="M${n(cx)} ${n(cy)} L${n(cx)} ${n(cy - r + 1)} A${n(r - 1)} ${n(r - 1)} 0 ${frac > 0.5 ? 1 : 0} 1 ${n(ex)} ${n(ey)} Z" fill="#1c7ed6"/>`;
      }
      return `<g class="icon" data-act="marker" data-marker="${m}"><circle cx="${n(cx)}" cy="${n(cy)}" r="${n(r - 1)}" fill="#fff" stroke="#1c7ed6" stroke-width="1.5"/>${pie}</g>`;
    }
    if (m.startsWith('flag-')) {
      const c = T.FLAG_COLORS[m.slice(5)] || '#e03131';
      return `<g class="icon" data-act="marker" data-marker="${m}"><path d="M${n(x + s * 0.22)} ${n(y + s * 0.95)} V${n(y + s * 0.08)}" stroke="#555" stroke-width="1.4" stroke-linecap="round"/><path d="M${n(x + s * 0.24)} ${n(y + s * 0.1)} H${n(x + s * 0.88)} L${n(x + s * 0.72)} ${n(y + s * 0.33)} L${n(x + s * 0.88)} ${n(y + s * 0.56)} H${n(x + s * 0.24)} Z" fill="${c}"/></g>`;
    }
    if (m.startsWith('star-')) {
      const c = T.STAR_COLORS[m.slice(5)] || '#f59f00';
      return `<g class="icon" data-act="marker" data-marker="${m}"><polygon points="${star(cx, cy + 0.5, r)}" fill="${c}"/></g>`;
    }
    return `<g class="icon" data-act="marker" data-marker="${esc(m)}"><text x="${n(cx)}" y="${n(cy + 1)}" text-anchor="middle" dominant-baseline="central" font-size="${n(s * 0.86)}" font-family="'Apple Color Emoji','Segoe UI Emoji','Noto Color Emoji',sans-serif">${esc(m)}</text></g>`;
  }

  // ---------- node ----------
  function nodeSVG(t, opts) {
    const b = t.box, st = b.style;
    const isRoot = t.depth === 0 && !t.floating;
    let s = `<g class="node${t.floating && t.depth === 0 ? ' floating' : ''}" data-id="${t.id}" transform="translate(${n(t.x)} ${n(t.y)})">`;
    s += shapeSVG(t, isRoot);
    if (b.img) {
      s += `<image href="${esc(t.node.image.src)}" x="${n(b.imgPos.x)}" y="${n(b.imgPos.y)}" width="${n(b.img.w)}" height="${n(b.img.h)}" preserveAspectRatio="xMidYMid meet"/>`;
    }
    const row = b.row;
    const iconY = row.y + (row.h - b.iconSize) / 2;
    let ix = row.x;
    for (const ic of b.lead) {
      s += markerSVG(ic.kind, ic.val, ix, iconY, b.iconSize, st.color);
      ix += b.iconSize + b.iconGap;
    }
    // text
    const tx0 = row.x + row.textX;
    const anchor = st.align === 'left' ? 'start' : st.align === 'right' ? 'end' : 'middle';
    const tx = st.align === 'left' ? tx0 : st.align === 'right' ? tx0 + row.textW : tx0 + row.textW / 2;
    const ty0 = row.y + (row.h - row.textH) / 2;
    const deco = [st.underline ? 'underline' : '', st.strike || t.node.task === 'done' && opts.strikeDone ? 'line-through' : ''].filter(Boolean).join(' ');
    const placeholder = !t.node.text;
    s += `<text class="label" font-family="${esc(st.font)}" font-size="${st.fontSize}" font-weight="${st.bold ? 700 : 400}"${st.italic ? ' font-style="italic"' : ''}${deco ? ` text-decoration="${deco}"` : ''} fill="${st.color}"${placeholder ? ' opacity="0.35"' : ''} text-anchor="${anchor}">`;
    const lines = placeholder ? [opts.export ? '' : 'Topic'] : b.lines;
    lines.forEach((ln, i) => {
      s += `<tspan x="${n(tx)}" y="${n(ty0 + i * b.lh + b.lh / 2)}" dominant-baseline="central">${esc(ln) || ' '}</tspan>`;
    });
    s += '</text>';
    ix = row.x + row.textX + row.textW + b.iconGap;
    for (const ic of b.trail) {
      s += markerSVG(ic.kind, ic.val, ix, iconY, b.iconSize, st.color);
      ix += b.iconSize + b.iconGap;
    }
    // tags
    if (b.tags.length) {
      let gx = b.tagsX;
      for (const tg of b.tags) {
        s += `<g class="tag"><rect x="${n(gx)}" y="${n(b.tagsY)}" width="${tg.w}" height="18" rx="9" fill="${st.line}" fill-opacity="0.18"/><text x="${n(gx + tg.w / 2)}" y="${n(b.tagsY + 9.5)}" text-anchor="middle" dominant-baseline="central" font-size="11" font-weight="600" font-family="${esc(st.font)}" fill="${st.color}" fill-opacity="0.85">${esc(tg.text)}</text></g>`;
        gx += tg.w + 4;
      }
    }
    // collapse / expand handle
    if (t.node.children.length) {
      const pos = togglePos(t);
      if (t.node.collapsed) {
        const cnt = countAll(t.node);
        s += `<g class="badge" data-act="toggle"><circle cx="${n(pos.x)}" cy="${n(pos.y)}" r="10" fill="${opts.bg}" stroke="${st.line}" stroke-width="1.6"/><text x="${n(pos.x)}" y="${n(pos.y + 0.5)}" text-anchor="middle" dominant-baseline="central" font-size="${cnt > 99 ? 8 : 10}" font-weight="700" fill="${st.line}" font-family="-apple-system,'Segoe UI',Arial,sans-serif">${cnt}</text></g>`;
      } else if (!opts.export && !(isRoot && opts.layout === 'mindmap')) {
        s += `<g class="toggler" data-act="toggle"><circle cx="${n(pos.x)}" cy="${n(pos.y)}" r="7" fill="${opts.bg}" stroke="${st.line}" stroke-width="1.4"/><path d="M${n(pos.x - 3.5)} ${n(pos.y)} H${n(pos.x + 3.5)}" stroke="${st.line}" stroke-width="1.6" stroke-linecap="round"/></g>`;
      }
    }
    s += '</g>';
    return s;
  }

  function countAll(node) { let c = 0; const r = (x) => { for (const k of x.children) { c++; r(k); } }; r(node); return c; }

  function togglePos(t) {
    const yb = t.box.style.shape === 'underline' ? t.h : t.h / 2;
    switch (t.dir) {
      case 'l': return { x: -12, y: yb };
      case 'd': return { x: t.w / 2, y: t.h + 12 };
      case 'u': return { x: t.w / 2, y: -12 };
      case 'tree': case 'tree-up': case 'fish-row': case 'fish-up': case 'fish-down': return { x: -12, y: t.h / 2 };
      case 'radial': return { x: t.w + 12, y: t.h / 2 };
      case 'both': return { x: t.w + 12, y: t.h / 2 };
      default: return { x: t.w + 12, y: yb };
    }
  }

  // ---------- full scene ----------
  /**
   * L: layout result; opts: { theme, settings, selected:Set, matches:Set, selectedRel, export, relations }
   */
  function scene(L, doc, opts) {
    const theme = opts.theme, s = opts.settings;
    const o = Object.assign({ bg: theme.bg, layout: s.layout }, opts);
    const parts = [];
    // boundaries
    for (const b of L.boundaries) {
      parts.push(`<g class="boundary" data-boundary="${b.id}"><rect x="${n(b.x)}" y="${n(b.y)}" width="${n(b.w)}" height="${n(b.h)}" rx="14" fill="${b.color}" fill-opacity="0.07" stroke="${b.color}" stroke-width="1.5" stroke-dasharray="6 5"/>`);
      if (b.label) parts.push(`<rect x="${n(b.x + 10)}" y="${n(b.y + 6)}" width="${n(Math.min(b.w - 20, b.label.length * 7.5 + 20))}" height="20" rx="10" fill="${b.color}"/><text x="${n(b.x + 20)}" y="${n(b.y + 16.5)}" dominant-baseline="central" font-size="12" font-weight="600" fill="${ML.themes.contrastText(b.color)}" font-family="${esc(ML.themes.getFont(s.font))}">${esc(b.label)}</text>`);
      parts.push('</g>');
    }
    // structural extras
    for (const e of L.extras) {
      if (e.type === 'spine') {
        parts.push(`<path d="M${n(e.x1)} ${n(e.y)} H${n(e.x2)}" stroke="${theme.line}" stroke-width="${s.lineWidth * 2.4}" stroke-linecap="round"/>`);
      } else if (e.type === 'axis') {
        parts.push(`<path d="M${n(e.x1)} ${n(e.y)} H${n(e.x2)}" stroke="${theme.line}" stroke-width="${s.lineWidth * 2}" stroke-linecap="round"/><path d="M${n(e.x2 - 2)} ${n(e.y - 7)} L${n(e.x2 + 10)} ${n(e.y)} L${n(e.x2 - 2)} ${n(e.y + 7)} Z" fill="${theme.line}"/>`);
      }
    }
    // connectors
    parts.push('<g class="edges">');
    for (const e of L.edges) {
      if (e.orient === 'axisdot') {
        parts.push(`<circle cx="${n(e.p1.x)}" cy="${n(e.p1.y)}" r="${s.lineWidth + 2.5}" fill="${e.color}"/>`);
        continue;
      }
      const p = ML.layout.edgePath(e, e.orient === 'bone' ? 'straight' : s.lineStyle);
      const w = e.orient === 'bone' ? e.width * 1.4 : e.width;
      if (p.filled) parts.push(`<path d="${p.d}" fill="${e.color}"/>`);
      else parts.push(`<path d="${p.d}" fill="none" stroke="${e.color}" stroke-width="${n(w)}" stroke-linecap="round" stroke-linejoin="round"/>`);
    }
    parts.push('</g>');
    // nodes
    parts.push('<g class="nodes">');
    for (const t of L.list) parts.push(nodeSVG(t, o));
    parts.push('</g>');
    // relationships
    parts.push('<g class="relations">');
    for (const r of doc.relations || []) {
      const a = L.nodes.get(r.from), b = L.nodes.get(r.to);
      if (!a || !b) continue;
      const rp = ML.layout.relationPath(a, b, r);
      const col = r.color || (theme.dark ? '#f8f9fa' : '#5c677d');
      const sel = o.selectedRel === r.id;
      const ah = 9, ang = rp.ang;
      const ax = rp.p2.x, ay = rp.p2.y;
      const p1 = `${n(ax - Math.cos(ang - 0.45) * ah)},${n(ay - Math.sin(ang - 0.45) * ah)}`;
      const p2 = `${n(ax - Math.cos(ang + 0.45) * ah)},${n(ay - Math.sin(ang + 0.45) * ah)}`;
      parts.push(`<g class="relation${sel ? ' selected' : ''}" data-rel="${r.id}"><path d="${rp.d}" fill="none" stroke="transparent" stroke-width="12"/><path d="${rp.d}" fill="none" stroke="${sel ? '#3b82f6' : col}" stroke-width="${sel ? 2.4 : 1.7}" stroke-dasharray="7 5"/><polygon points="${n(ax)},${n(ay)} ${p1} ${p2}" fill="${sel ? '#3b82f6' : col}"/>`);
      if (r.label) {
        const lw = r.label.length * 7 + 16;
        parts.push(`<rect x="${n(rp.mid.x - lw / 2)}" y="${n(rp.mid.y - 11)}" width="${n(lw)}" height="22" rx="11" fill="${theme.bg}" stroke="${col}" stroke-width="1"/><text x="${n(rp.mid.x)}" y="${n(rp.mid.y + 0.5)}" text-anchor="middle" dominant-baseline="central" font-size="12" fill="${col}" font-family="${esc(ML.themes.getFont(s.font))}">${esc(r.label)}</text>`);
      }
      parts.push('</g>');
    }
    parts.push('</g>');
    // selection / search overlays (live canvas only)
    if (!o.export) {
      parts.push('<g class="overlay">');
      if (o.matches) for (const id of o.matches) {
        const t = L.nodes.get(id);
        if (t) parts.push(`<rect x="${n(t.x - 5)}" y="${n(t.y - 5)}" width="${n(t.w + 10)}" height="${n(t.h + 10)}" rx="10" fill="none" stroke="#fab005" stroke-width="3" stroke-dasharray="5 3"/>`);
      }
      if (o.selected) for (const id of o.selected) {
        const t = L.nodes.get(id);
        if (t) parts.push(`<rect class="sel" x="${n(t.x - 4.5)}" y="${n(t.y - 4.5)}" width="${n(t.w + 9)}" height="${n(t.h + 9)}" rx="10" fill="none" stroke="#3b82f6" stroke-width="2.5"/>`);
      }
      parts.push('</g>');
    }
    return parts.join('');
  }

  /** Standalone SVG document for export. */
  function exportSVG(L, doc, opts) {
    const theme = opts.theme, s = opts.settings;
    const pad = opts.padding == null ? 40 : opts.padding;
    const b = L.bounds;
    const x = Math.floor(b.minX - pad), y = Math.floor(b.minY - pad);
    const w = Math.ceil(b.w + pad * 2), h = Math.ceil(b.h + pad * 2);
    const tile = opts.transparent ? null : patternTile(s.pattern, theme.ink);
    let defs = '';
    let bg = opts.transparent ? '' : `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${theme.bg}"/>`;
    if (tile) {
      defs = `<defs><pattern id="ml-bg" patternUnits="userSpaceOnUse" width="${tile.size}" height="${tile.h || tile.size}">${tile.body}</pattern></defs>`;
      bg += `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="url(#ml-bg)"/>`;
    }
    const body = scene(L, doc, Object.assign({}, opts, { export: true, selected: null, matches: null, selectedRel: null }));
    return {
      svg: `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${w}" height="${h}" viewBox="${x} ${y} ${w} ${h}">${defs}${bg}${body}</svg>`,
      width: w, height: h,
    };
  }

  ML.render = { scene, exportSVG, patternCSS, patternTile, markerSVG, esc };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
