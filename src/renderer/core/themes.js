/* Mindloom — themes, shapes, layouts, patterns, markers (pure data + helpers) */
(function (ML) {
  'use strict';

  const R = (id, name, dark, o) => Object.assign({ id, name, dark }, o);

  // Each theme: canvas background, pattern ink, level styles, line colour, branch palette.
  const THEMES = [
    R('classic', 'Classic', false, {
      bg: '#f6f7fb', ink: 'rgba(40,50,90,0.10)',
      root: { fill: '#2b3a67', color: '#ffffff', border: '#2b3a67' },
      main: { fill: '#e6ebf7', color: '#1d2440', border: '#b9c4e3' },
      sub: { fill: '#ffffff', color: '#2a2f45', border: '#c9d1ea' },
      line: '#8d99c2', rainbow: false,
      branches: ['#4c6ef5', '#12b886', '#fab005', '#fa5252', '#7950f2', '#15aabf', '#fd7e14', '#e64980'],
    }),
    R('fresh', 'Fresh Rainbow', false, {
      bg: '#ffffff', ink: 'rgba(0,0,0,0.07)',
      root: { fill: '#1f2937', color: '#ffffff', border: '#1f2937' },
      main: { fill: '#ffffff', color: '#ffffff', border: '#ffffff' },
      sub: { fill: '#ffffff', color: '#1f2937', border: '#d1d5db' },
      line: '#9ca3af', rainbow: true,
      branches: ['#ef4444', '#d97706', '#10b981', '#3b82f6', '#8b5cf6', '#ec4899', '#0d9488', '#ea580c'],
    }),
    R('ocean', 'Ocean', false, {
      bg: '#eef7fb', ink: 'rgba(14,90,130,0.10)',
      root: { fill: '#0b4f6c', color: '#ffffff', border: '#0b4f6c' },
      main: { fill: '#01baef', color: '#ffffff', border: '#01baef' },
      sub: { fill: '#ffffff', color: '#0b3c52', border: '#9fd8ee' },
      line: '#20a4d4', rainbow: false,
      branches: ['#0077b6', '#00b4d8', '#48cae4', '#0096c7', '#023e8a', '#90e0ef', '#03045e', '#0aa1a1'],
    }),
    R('sunset', 'Sunset', false, {
      bg: '#fff7f0', ink: 'rgba(160,70,20,0.10)',
      root: { fill: '#9d0208', color: '#ffffff', border: '#9d0208' },
      main: { fill: '#f48c06', color: '#ffffff', border: '#f48c06' },
      sub: { fill: '#fffaf5', color: '#5c1d04', border: '#f7c59f' },
      line: '#e85d04', rainbow: true,
      branches: ['#d00000', '#dc2f02', '#e85d04', '#f48c06', '#faa307', '#9d0208', '#6a040f', '#ffba08'],
    }),
    R('forest', 'Forest', false, {
      bg: '#f3f8f1', ink: 'rgba(40,90,40,0.10)',
      root: { fill: '#2d6a4f', color: '#ffffff', border: '#2d6a4f' },
      main: { fill: '#74c69d', color: '#0b2e1f', border: '#74c69d' },
      sub: { fill: '#ffffff', color: '#1b4332', border: '#b7e4c7' },
      line: '#52b788', rainbow: false,
      branches: ['#40916c', '#52b788', '#95d5b2', '#1b4332', '#74c69d', '#2d6a4f', '#b7e4c7', '#081c15'],
    }),
    R('lavender', 'Lavender', false, {
      bg: '#f8f5ff', ink: 'rgba(90,60,160,0.10)',
      root: { fill: '#5a189a', color: '#ffffff', border: '#5a189a' },
      main: { fill: '#e0c3fc', color: '#3c096c', border: '#c8a2f4' },
      sub: { fill: '#ffffff', color: '#3c096c', border: '#dcc6f7' },
      line: '#9d4edd', rainbow: false,
      branches: ['#7b2cbf', '#9d4edd', '#c77dff', '#5a189a', '#e0aaff', '#3c096c', '#b5179e', '#7209b7'],
    }),
    R('candy', 'Candy', false, {
      bg: '#fff5fa', ink: 'rgba(200,60,130,0.10)',
      root: { fill: '#ff4d8d', color: '#ffffff', border: '#ff4d8d' },
      main: { fill: '#ffffff', color: '#ffffff', border: '#ffffff' },
      sub: { fill: '#ffffff', color: '#5b2142', border: '#ffc2dc' },
      line: '#ff8fb8', rainbow: true,
      branches: ['#ff6b9a', '#ffb347', '#7bdcb5', '#8ed1fc', '#b28dff', '#ffd166', '#06d6a0', '#ef476f'],
    }),
    R('pastel', 'Pastel', false, {
      bg: '#fbfaf7', ink: 'rgba(0,0,0,0.06)',
      root: { fill: '#ffd6a5', color: '#5c3d1e', border: '#f6bd7f' },
      main: { fill: '#caffbf', color: '#24452a', border: '#a9e6a0' },
      sub: { fill: '#ffffff', color: '#3d3d3d', border: '#e6e1d6' },
      line: '#c9b8a6', rainbow: false,
      branches: ['#ffadad', '#ffd6a5', '#fdffb6', '#caffbf', '#9bf6ff', '#a0c4ff', '#bdb2ff', '#ffc6ff'],
    }),
    R('business', 'Business', false, {
      bg: '#ffffff', ink: 'rgba(0,0,0,0.07)',
      root: { fill: '#111827', color: '#ffffff', border: '#111827' },
      main: { fill: '#f3f4f6', color: '#111827', border: '#d1d5db' },
      sub: { fill: '#ffffff', color: '#374151', border: '#e5e7eb' },
      line: '#6b7280', rainbow: false,
      branches: ['#1e40af', '#047857', '#b45309', '#b91c1c', '#6d28d9', '#0e7490', '#be185d', '#4d7c0f'],
    }),
    R('mono', 'Monochrome', false, {
      bg: '#fafafa', ink: 'rgba(0,0,0,0.08)',
      root: { fill: '#000000', color: '#ffffff', border: '#000000' },
      main: { fill: '#ffffff', color: '#000000', border: '#000000' },
      sub: { fill: '#ffffff', color: '#222222', border: '#9a9a9a' },
      line: '#444444', rainbow: false,
      branches: ['#000000', '#333333', '#555555', '#777777', '#222222', '#444444', '#666666', '#888888'],
    }),
    R('sakura', 'Sakura', false, {
      bg: '#fff8f9', ink: 'rgba(190,80,110,0.09)',
      root: { fill: '#c9184a', color: '#ffffff', border: '#c9184a' },
      main: { fill: '#ffccd5', color: '#590d22', border: '#ffb3c1' },
      sub: { fill: '#ffffff', color: '#590d22', border: '#ffccd5' },
      line: '#ff758f', rainbow: false,
      branches: ['#ff4d6d', '#ff758f', '#c9184a', '#ff8fa3', '#a4133c', '#ffb3c1', '#800f2f', '#ff4d6d'],
    }),
    R('earth', 'Coffee', false, {
      bg: '#f7f1e8', ink: 'rgba(100,70,40,0.10)',
      root: { fill: '#583101', color: '#fff8ee', border: '#583101' },
      main: { fill: '#bc8a5f', color: '#ffffff', border: '#bc8a5f' },
      sub: { fill: '#fffaf2', color: '#4a2c0a', border: '#dfc2a0' },
      line: '#a47148', rainbow: false,
      branches: ['#8b5e34', '#a47148', '#bc8a5f', '#6f4518', '#d4a276', '#603808', '#e7bc91', '#7f5539'],
    }),
    R('midnight', 'Midnight', true, {
      bg: '#0f172a', ink: 'rgba(148,163,184,0.12)',
      root: { fill: '#6366f1', color: '#ffffff', border: '#6366f1' },
      main: { fill: '#1e293b', color: '#e2e8f0', border: '#334155' },
      sub: { fill: '#0f172a', color: '#cbd5e1', border: '#334155' },
      line: '#64748b', rainbow: true,
      branches: ['#818cf8', '#34d399', '#fbbf24', '#f87171', '#a78bfa', '#22d3ee', '#fb923c', '#f472b6'],
    }),
    R('neon', 'Neon Night', true, {
      bg: '#0b0b12', ink: 'rgba(0,255,200,0.08)',
      root: { fill: '#0b0b12', color: '#39ff14', border: '#39ff14' },
      main: { fill: '#0b0b12', color: '#ffffff', border: '#00e5ff' },
      sub: { fill: '#0b0b12', color: '#d0d0ff', border: '#3a3a5a' },
      line: '#00e5ff', rainbow: true,
      branches: ['#00e5ff', '#ff00e5', '#39ff14', '#ffe600', '#ff3b3b', '#9d4bff', '#00ffa3', '#ff8a00'],
    }),
    R('dracula', 'Vampire', true, {
      bg: '#21222c', ink: 'rgba(248,248,242,0.07)',
      root: { fill: '#bd93f9', color: '#21222c', border: '#bd93f9' },
      main: { fill: '#44475a', color: '#f8f8f2', border: '#6272a4' },
      sub: { fill: '#282a36', color: '#f8f8f2', border: '#44475a' },
      line: '#6272a4', rainbow: true,
      branches: ['#ff79c6', '#50fa7b', '#f1fa8c', '#8be9fd', '#ffb86c', '#ff5555', '#bd93f9', '#6272a4'],
    }),
    R('chalk', 'Chalkboard', true, {
      bg: '#26332b', ink: 'rgba(255,255,255,0.08)',
      root: { fill: '#26332b', color: '#ffffff', border: '#ffffff' },
      main: { fill: '#26332b', color: '#fdf6e3', border: '#e8e0c8' },
      sub: { fill: '#26332b', color: '#e8e0c8', border: '#7c8a80' },
      line: '#e8e0c8', rainbow: false,
      branches: ['#ffd6e0', '#c1f0c1', '#fff3b0', '#b8e0ff', '#e0c3fc', '#ffc9a8', '#a8f0e8', '#ffffff'],
    }),
  ];

  const LAYOUTS = [
    { id: 'mindmap', name: 'Mind Map (balanced)' },
    { id: 'right', name: 'Logic Chart → Right' },
    { id: 'left', name: 'Logic Chart ← Left' },
    { id: 'down', name: 'Org Chart ↓ Down' },
    { id: 'up', name: 'Org Chart ↑ Up' },
    { id: 'tree', name: 'Tree / Outline' },
    { id: 'timeline', name: 'Timeline' },
    { id: 'fishbone', name: 'Fishbone' },
    { id: 'radial', name: 'Radial' },
  ];

  const LINE_STYLES = [
    { id: 'curve', name: 'Curved' },
    { id: 'tapered', name: 'Tapered branch' },
    { id: 'straight', name: 'Straight' },
    { id: 'elbow', name: 'Elbow' },
    { id: 'rounded', name: 'Rounded elbow' },
  ];

  const SHAPES = [
    { id: 'rounded', name: 'Rounded' },
    { id: 'rect', name: 'Rectangle' },
    { id: 'pill', name: 'Capsule' },
    { id: 'ellipse', name: 'Ellipse' },
    { id: 'circle', name: 'Circle' },
    { id: 'underline', name: 'Underline' },
    { id: 'diamond', name: 'Diamond' },
    { id: 'hexagon', name: 'Hexagon' },
    { id: 'parallelogram', name: 'Parallelogram' },
    { id: 'cloud', name: 'Cloud' },
    { id: 'none', name: 'Text only' },
  ];

  const PATTERNS = [
    { id: 'none', name: 'Plain' },
    { id: 'dots', name: 'Dots' },
    { id: 'grid', name: 'Grid' },
    { id: 'graph', name: 'Graph paper' },
    { id: 'lines', name: 'Ruled lines' },
    { id: 'cross', name: 'Crosses' },
    { id: 'diagonal', name: 'Diagonal' },
    { id: 'isometric', name: 'Isometric dots' },
  ];

  const FONTS = [
    { id: 'system', name: 'System', css: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif" },
    { id: 'rounded', name: 'Rounded', css: "ui-rounded, 'SF Pro Rounded', 'Arial Rounded MT Bold', 'Segoe UI', sans-serif" },
    { id: 'serif', name: 'Serif', css: "Georgia, 'Times New Roman', 'Noto Serif', serif" },
    { id: 'mono', name: 'Monospace', css: "ui-monospace, 'SF Mono', Menlo, Consolas, 'Courier New', monospace" },
    { id: 'hand', name: 'Handwriting', css: "'Bradley Hand', 'Segoe Print', 'Comic Sans MS', 'Chalkboard SE', cursive" },
    { id: 'condensed', name: 'Condensed', css: "'Avenir Next Condensed', 'Arial Narrow', 'Roboto Condensed', sans-serif" },
  ];

  // Map style presets = structure + line + shapes in one click.
  const MAP_STYLES = [
    { id: 'classic', name: 'Classic', layout: 'mindmap', lineStyle: 'curve', shapes: { root: 'rounded', main: 'rounded', sub: 'rounded' } },
    { id: 'organic', name: 'Organic', layout: 'mindmap', lineStyle: 'tapered', shapes: { root: 'ellipse', main: 'pill', sub: 'underline' } },
    { id: 'clean', name: 'Clean Logic', layout: 'right', lineStyle: 'rounded', shapes: { root: 'rounded', main: 'rect', sub: 'underline' } },
    { id: 'org', name: 'Org Chart', layout: 'down', lineStyle: 'elbow', shapes: { root: 'rect', main: 'rect', sub: 'rect' } },
    { id: 'bubbles', name: 'Bubbles', layout: 'radial', lineStyle: 'straight', shapes: { root: 'circle', main: 'ellipse', sub: 'pill' } },
    { id: 'outline', name: 'Outline', layout: 'tree', lineStyle: 'rounded', shapes: { root: 'rounded', main: 'none', sub: 'none' } },
    { id: 'timeline', name: 'Timeline', layout: 'timeline', lineStyle: 'rounded', shapes: { root: 'pill', main: 'pill', sub: 'underline' } },
    { id: 'fishbone', name: 'Fishbone', layout: 'fishbone', lineStyle: 'straight', shapes: { root: 'rounded', main: 'rounded', sub: 'underline' } },
    { id: 'sketch', name: 'Whiteboard', layout: 'mindmap', lineStyle: 'curve', shapes: { root: 'cloud', main: 'hexagon', sub: 'underline' }, font: 'hand' },
  ];

  // Markers: priority, progress, flags, stars, plus emoji stickers.
  const MARKER_GROUPS = [
    { name: 'Priority', items: ['p1', 'p2', 'p3', 'p4', 'p5', 'p6'] },
    { name: 'Progress', items: ['pr0', 'pr25', 'pr50', 'pr75', 'pr100'] },
    { name: 'Flags', items: ['flag-red', 'flag-orange', 'flag-green', 'flag-blue', 'flag-purple'] },
    { name: 'Stars', items: ['star-yellow', 'star-red', 'star-blue', 'star-green'] },
    { name: 'Symbols', items: ['✅', '❌', '❓', '❗', '💡', '⭐', '🔥', '🎯', '📌', '📅', '💰', '⚠️', '👍', '👎', '🚀', '❤️', '🏆', '🔒', '📈', '🧠', '⏰', '📎', '🐞', '✏️'] },
  ];
  const PRIORITY_COLORS = ['#e03131', '#f76707', '#f59f00', '#2f9e44', '#1c7ed6', '#7048e8'];
  const FLAG_COLORS = { red: '#e03131', orange: '#f76707', green: '#2f9e44', blue: '#1c7ed6', purple: '#7048e8' };
  const STAR_COLORS = { yellow: '#f59f00', red: '#e03131', blue: '#1c7ed6', green: '#2f9e44' };
  const markerGroupOf = (m) => (/^p\d$/.test(m) ? 'p' : /^pr\d+$/.test(m) ? 'pr' : m.startsWith('flag-') ? 'flag' : m.startsWith('star-') ? 'star' : 'emoji:' + m);

  const SWATCHES = ['#ffffff', '#f1f3f5', '#ced4da', '#495057', '#000000', '#ffe3e3', '#ff8787', '#e03131',
    '#fff4e6', '#ffa94d', '#f76707', '#fff9db', '#ffd43b', '#f59f00', '#ebfbee', '#69db7c', '#2f9e44',
    '#e3fafc', '#3bc9db', '#0c8599', '#e7f5ff', '#4dabf7', '#1c7ed6', '#f3f0ff', '#9775fa', '#6741d9',
    '#fff0f6', '#f783ac', '#c2255c', 'transparent'];

  const DEFAULT_SETTINGS = {
    theme: 'fresh', layout: 'mindmap', lineStyle: 'curve', pattern: 'dots', font: 'system',
    shapes: { root: 'rounded', main: 'rounded', sub: 'rounded' },
    rainbow: null, // null = theme default
    numbering: 'off', // 'on' = automatic outline numbering (1, 1.1, 1.1.1…)
    hGap: 56, vGap: 14, lineWidth: 2, maxWidth: 260, fontScale: 1,
  };

  function getTheme(id) { return THEMES.find((t) => t.id === id) || THEMES[0]; }
  function getFont(id) { return (FONTS.find((f) => f.id === id) || FONTS[0]).css; }

  // Relative luminance → readable text colour on a fill.
  function hexToRgb(hex) {
    if (!hex || hex[0] !== '#') return null;
    let h = hex.slice(1);
    if (h.length === 3) h = h.split('').map((c) => c + c).join('');
    const n = parseInt(h.slice(0, 6), 16);
    if (Number.isNaN(n)) return null;
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  function luminance(hex) {
    const rgb = hexToRgb(hex);
    if (!rgb) return 1;
    const [r, g, b] = rgb.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }
  function contrastText(hex, dark = '#1a1a1a', light = '#ffffff') { return luminance(hex) > 0.42 ? dark : light; }

  /**
   * Resolve the visual style of a node.
   * depth 0 = root/floating root, 1 = main topic, 2+ = sub topic.
   */
  function resolveStyle(node, depth, branchIdx, theme, settings, isFloating) {
    const cd = isFloating ? depth + 1 : depth; // a floating topic is styled like a main topic
    const lvl = cd === 0 ? 'root' : cd === 1 ? 'main' : 'sub';
    const base = theme[lvl];
    const rainbow = settings.rainbow == null ? theme.rainbow : settings.rainbow;
    const bcol = theme.branches[(branchIdx < 0 ? 0 : branchIdx) % theme.branches.length];
    let fill = base.fill, color = base.color, border = base.border, line = theme.line;
    if (rainbow && cd >= 1) {
      line = bcol;
      if (cd === 1) {
        if (theme.id === 'neon' || theme.id === 'chalk') { border = bcol; color = bcol; }
        else { fill = bcol; border = bcol; color = contrastText(bcol); }
      } else border = bcol;
    } else if (!rainbow && cd === 1 && base.fill === base.color) {
      // themes whose main topics rely on rainbow colours need a fallback when rainbow is off
      fill = theme.dark ? '#1f2937' : '#f3f4f6'; color = theme.dark ? '#f9fafb' : '#111827'; border = theme.line;
    }
    const st = node.style || {};
    const fs = Math.round((cd === 0 ? 24 : cd === 1 ? 17 : 14) * (settings.fontScale || 1));
    const shape = st.shape || (settings.shapes && settings.shapes[lvl]) || 'rounded';
    if (st.fill && !st.color && st.fill !== 'transparent') color = contrastText(st.fill);
    return {
      level: lvl,
      fill: st.fill || fill, color: st.color || color, border: st.border || border,
      borderWidth: st.borderWidth != null ? st.borderWidth : (depth === 0 && !isFloating ? 0 : theme.dark ? 1.5 : 1.2),
      borderDash: st.borderDash || 'solid',
      line: st.lineColor || line,
      shape,
      fontSize: st.fontSize || fs,
      bold: st.bold != null ? st.bold : cd <= 1,
      italic: !!st.italic, underline: !!st.underline, strike: !!st.strike,
      align: st.align || 'center',
      font: getFont(st.font || settings.font),
    };
  }

  ML.themes = {
    THEMES, LAYOUTS, LINE_STYLES, SHAPES, PATTERNS, FONTS, MAP_STYLES, MARKER_GROUPS, SWATCHES,
    PRIORITY_COLORS, FLAG_COLORS, STAR_COLORS, DEFAULT_SETTINGS,
    getTheme, getFont, resolveStyle, contrastText, luminance, markerGroupOf,
  };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
