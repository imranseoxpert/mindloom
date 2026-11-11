'use strict';
/* Mindloom — Electron main process: windows, menus, file dialogs, PDF export, AI assistant */
const { app, BrowserWindow, Menu, dialog, ipcMain, shell, safeStorage, nativeTheme } = require('electron');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');

const isMac = process.platform === 'darwin';
const OPEN_EXT = ['mindloom', 'json', 'md', 'markdown', 'txt', 'opml', 'mm', 'xmind'];
const windows = new Set();
const state = new WeakMap(); // win -> {dirty, path, title, forceClose, initialPath}
let pendingOpen = [];
let firstWindowCreated = false;

// self-test runs use a throwaway profile so they never touch the user's maps or settings
if (process.env.MINDLOOM_SMOKE) app.setPath('userData', fs.mkdtempSync(path.join(os.tmpdir(), 'mindloom-profile-')));

// ---------------- recent files ----------------
const recentFile = () => path.join(app.getPath('userData'), 'recent.json');
function getRecent() {
  try { return JSON.parse(fs.readFileSync(recentFile(), 'utf8')).filter((p) => typeof p === 'string'); } catch (e) { return []; }
}
function addRecent(p) {
  if (!p) return;
  const list = [p].concat(getRecent().filter((x) => x !== p)).slice(0, 12);
  try { fs.writeFileSync(recentFile(), JSON.stringify(list)); } catch (e) { /* ignore */ }
  app.addRecentDocument(p);
  buildMenu();
}

// ---------------- file helpers ----------------
function readDocFile(p) {
  const name = path.basename(p);
  if (/\.xmind$/i.test(p)) return { name, path: p, base64: fs.readFileSync(p).toString('base64') };
  return { name, path: p, text: fs.readFileSync(p, 'utf8') };
}
function fileArg(argv) {
  const args = argv.slice(app.isPackaged ? 1 : 2);
  return args.find((a) => !a.startsWith('-') && OPEN_EXT.some((e) => a.toLowerCase().endsWith('.' + e)) && fs.existsSync(a)) || null;
}

// ---------------- windows ----------------
function createWindow(initialPath) {
  const isFirst = !firstWindowCreated;
  firstWindowCreated = true;
  const win = new BrowserWindow({
    width: 1440, height: 920, minWidth: 860, minHeight: 560,
    show: false,
    title: 'Mindloom',
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#1d1f24' : '#ffffff',
    titleBarStyle: isMac ? 'hiddenInset' : 'default',
    trafficLightPosition: { x: 16, y: 19 },
    icon: path.join(__dirname, '..', '..', 'build', 'icon.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      spellcheck: true,
      additionalArguments: [`--ml-first=${isFirst ? 1 : 0}`, `--ml-version=${app.getVersion()}`],
    },
  });
  windows.add(win);
  state.set(win, { dirty: false, path: null, title: 'Mindloom', forceClose: false, initialPath: initialPath || null });
  win.loadFile(path.join(__dirname, '..', 'renderer', 'index.html'));
  win.once('ready-to-show', () => { if (process.env.MINDLOOM_SMOKE) win.showInactive(); else win.show(); });

  // never navigate away from the app; open external links in the browser
  win.webContents.on('will-navigate', (e, url) => { e.preventDefault(); if (/^https?:/i.test(url)) shell.openExternal(url); });
  win.webContents.setWindowOpenHandler(({ url }) => { if (/^(https?:|mailto:)/i.test(url)) shell.openExternal(url); return { action: 'deny' }; });

  win.on('close', async (e) => {
    const st = state.get(win);
    if (!st || st.forceClose || !st.dirty || process.env.MINDLOOM_SMOKE) return;
    e.preventDefault();
    const { response } = await dialog.showMessageBox(win, {
      type: 'warning', buttons: ['Save', "Don't Save", 'Cancel'], defaultId: 0, cancelId: 2,
      message: `Do you want to save the changes you made to “${st.title.replace(/ •? — Mindloom$/, '')}”?`,
      detail: "Your changes will be lost if you don't save them.",
    });
    if (response === 0) send(win, 'saveThenClose');
    else if (response === 1) {
      st.forceClose = true;
      try { await win.webContents.executeJavaScript("localStorage.removeItem('mindloom:autosave')"); } catch (err) { /* ignore */ }
      win.close();
    }
  });
  win.on('closed', () => windows.delete(win));
  return win;
}

function send(win, cmd, arg) { if (win && !win.isDestroyed()) win.webContents.send('menu', cmd, arg); }
function focused() { return BrowserWindow.getFocusedWindow() || [...windows][0] || null; }
function openInWindow(p) {
  const w = focused();
  if (w) { w.webContents.send('open-path', p); w.show(); w.focus(); } else createWindow(p);
}

// ---------------- menu ----------------
function buildMenu() {
  // Shortcuts are handled by the page itself; menu items only display them (registerAccelerator: false)
  const c = (label, cmd, accel, extra) => Object.assign({ label, accelerator: accel, registerAccelerator: false, click: () => send(focused(), cmd) }, extra || {});
  const recent = getRecent();
  const template = [
    ...(isMac ? [{ role: 'appMenu', submenu: [
      { label: 'About Mindloom', click: () => send(focused(), 'about') },
      { type: 'separator' },
      { label: 'AI Assistant…', click: () => send(focused(), 'ai') },
      { type: 'separator' }, { role: 'services' }, { type: 'separator' },
      { role: 'hide' }, { role: 'hideOthers' }, { role: 'unhide' }, { type: 'separator' }, { role: 'quit' },
    ] }] : []),
    { label: 'File', submenu: [
      c('New from Template…', 'templates', 'CmdOrCtrl+N'),
      { label: 'New Window', accelerator: 'CmdOrCtrl+Shift+N', click: () => createWindow() },
      { type: 'separator' },
      c('Open…', 'open', 'CmdOrCtrl+O'),
      { label: 'Open Recent', submenu: recent.length ? recent.map((p) => ({ label: p, click: () => openInWindow(p) })).concat([{ type: 'separator' }, { label: 'Clear Recent', click: () => { try { fs.writeFileSync(recentFile(), '[]'); } catch (e) { /* ignore */ } app.clearRecentDocuments(); buildMenu(); } }]) : [{ label: 'No recent files', enabled: false }] },
      { type: 'separator' },
      c('Save', 'save', 'CmdOrCtrl+S'),
      c('Save As…', 'saveAs', 'CmdOrCtrl+Shift+S'),
      { type: 'separator' },
      c('Import (Markdown, OPML, XMind, FreeMind, Text)…', 'open'),
      { label: 'Export', submenu: [
        c('PNG Image…', 'exportPNG'), c('SVG Vector…', 'exportSVG'), c('PDF Document…', 'exportPDF'), { type: 'separator' },
        c('Markdown…', 'exportMarkdown'), c('OPML…', 'exportOPML'), c('Plain Text Outline…', 'exportText'), c('FreeMind (.mm)…', 'exportFreeMind'),
      ] },
      { type: 'separator' },
      isMac ? { role: 'close' } : { role: 'quit', label: 'Exit' },
    ] },
    { label: 'Edit', submenu: [
      // undo/redo are real accelerators: in text fields they undo typing, on the canvas they undo map changes
      { label: 'Undo', accelerator: 'CmdOrCtrl+Z', click: () => send(focused(), 'undo') },
      { label: 'Redo', accelerator: isMac ? 'Cmd+Shift+Z' : 'Ctrl+Y', click: () => send(focused(), 'redo') },
      { type: 'separator' },
      { role: 'cut' }, { role: 'copy' }, { role: 'paste' }, { role: 'selectAll' },
      { type: 'separator' },
      c('Duplicate Topic', 'duplicate', 'CmdOrCtrl+D'),
      c('Delete Topic', 'deleteSel', 'Delete'),
      { type: 'separator' },
      c('Find…', 'find', 'CmdOrCtrl+F'),
    ] },
    { label: 'Insert', submenu: [
      c('Subtopic', 'addChild', 'Tab'), c('Topic', 'addSibling', 'Enter'), c('Parent Topic', 'addParent', 'CmdOrCtrl+Enter'),
      { type: 'separator' },
      c('Relationship', 'relation', 'CmdOrCtrl+Shift+R'), c('Boundary', 'boundary', 'CmdOrCtrl+Shift+B'),
      c('Note', 'note', 'CmdOrCtrl+Shift+M'), c('Hyperlink…', 'link', 'CmdOrCtrl+K'), c('Image…', 'image'),
      c('Task Checkbox', 'task', 'CmdOrCtrl+Shift+T'), c('Markers…', 'markers'),
      { type: 'separator' },
      c('AI Assistant…', 'ai'),
    ] },
    { label: 'View', submenu: [
      c('Zoom In', 'zoomIn', 'CmdOrCtrl+='), c('Zoom Out', 'zoomOut', 'CmdOrCtrl+-'), c('Fit Map', 'fit', 'CmdOrCtrl+0'), c('Actual Size', 'actualSize'),
      { type: 'separator' },
      c('Focus on Branch', 'focus', 'CmdOrCtrl+Shift+F'), c('Presentation Mode', 'present', 'F5'),
      c('Expand All', 'expandAll'), c('Collapse All', 'collapseAll'),
      { type: 'separator' },
      c('Outline Panel', 'toggleOutline', 'CmdOrCtrl+\\'), c('Format Panel', 'toggleInspector', 'CmdOrCtrl+Shift+\\'), c('Mini Map', 'toggleMinimap'),
      c('Map Style & Theme…', 'mapPanel'), c('Light / Dark Interface', 'toggleUiTheme'),
      { type: 'separator' },
      { role: 'togglefullscreen' },
    ] },
    { role: 'windowMenu' },
    { role: 'help', submenu: [
      c('Keyboard Shortcuts', 'shortcuts', 'F1'),
      { label: 'Mindloom on GitHub', click: () => shell.openExternal(REPO_URL) },
      { type: 'separator' },
      { label: 'Toggle Developer Tools', accelerator: isMac ? 'Alt+Cmd+I' : 'Ctrl+Shift+I', click: () => { const w = focused(); if (w) w.webContents.toggleDevTools(); } },
      ...(!isMac ? [{ label: 'About Mindloom', click: () => send(focused(), 'about') }] : []),
    ] },
  ];
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}
const REPO_URL = (() => { try { const u = require('../../package.json').homepage; return u && /^https:\/\//.test(u) ? u : 'https://github.com/'; } catch (e) { return 'https://github.com/'; } })();

// ---------------- IPC ----------------
const winOf = (e) => BrowserWindow.fromWebContents(e.sender);

ipcMain.handle('get-initial-path', (e) => { const st = state.get(winOf(e)); const p = st && st.initialPath; if (st) st.initialPath = null; return p; });
ipcMain.handle('open-file', async (e) => {
  const r = await dialog.showOpenDialog(winOf(e), {
    properties: ['openFile'],
    filters: [{ name: 'Mind maps & outlines', extensions: OPEN_EXT }, { name: 'All files', extensions: ['*'] }],
  });
  if (r.canceled || !r.filePaths[0]) return null;
  const p = r.filePaths[0];
  if (/\.mindloom$/i.test(p)) addRecent(p);
  return readDocFile(p);
});
ipcMain.handle('read-path', (e, p) => {
  try {
    if (typeof p !== 'string' || !OPEN_EXT.some((x) => p.toLowerCase().endsWith('.' + x))) return { error: 'Unsupported file' };
    const r = readDocFile(p);
    if (/\.mindloom$/i.test(p)) addRecent(p);
    return r;
  } catch (err) { return { error: err.message }; }
});
ipcMain.handle('save-file', async (e, { path: p, text, defaultName }) => {
  let target = p;
  if (!target) {
    const r = await dialog.showSaveDialog(winOf(e), { defaultPath: defaultName || 'Untitled.mindloom', filters: [{ name: 'Mindloom map', extensions: ['mindloom'] }] });
    if (r.canceled || !r.filePath) return null;
    target = r.filePath;
    if (!/\.mindloom$/i.test(target)) target += '.mindloom';
  }
  // write atomically so a crash mid-save never corrupts the map
  const tmp = target + '.tmp-' + process.pid;
  fs.writeFileSync(tmp, text, 'utf8');
  fs.renameSync(tmp, target);
  addRecent(target);
  return { path: target, name: path.basename(target) };
});
ipcMain.handle('export-file', async (e, { defaultName, ext, label, base64, text }) => {
  const r = await dialog.showSaveDialog(winOf(e), { defaultPath: defaultName, filters: [{ name: label || ext.toUpperCase(), extensions: [ext] }] });
  if (r.canceled || !r.filePath) return null;
  if (base64 != null) fs.writeFileSync(r.filePath, Buffer.from(base64, 'base64'));
  else fs.writeFileSync(r.filePath, text, 'utf8');
  return { path: r.filePath };
});

async function svgToPdf(svg, width, height) {
  const maxIn = 199; // PDF pages are limited to 200 inches
  const k = Math.min(1, (maxIn * 96) / width, (maxIn * 96) / height);
  const w = Math.ceil(width * k), h = Math.ceil(height * k);
  const html = `<!doctype html><meta charset="utf-8"><style>@page{margin:0}html,body{margin:0;padding:0}svg{display:block;width:${w}px;height:${h}px}</style>${svg}`;
  const tmp = path.join(os.tmpdir(), `mindloom-pdf-${process.pid}-${Date.now()}.html`);
  fs.writeFileSync(tmp, html, 'utf8');
  const pw = new BrowserWindow({ show: false, width: Math.min(w, 4000), height: Math.min(h, 4000), webPreferences: { sandbox: true, javascript: false } });
  try {
    await pw.loadFile(tmp);
    return await pw.webContents.printToPDF({ printBackground: true, pageSize: { width: w / 96, height: h / 96 }, margins: { top: 0, bottom: 0, left: 0, right: 0 } });
  } finally {
    pw.destroy();
    fs.rm(tmp, () => {});
  }
}
module.exports.svgToPdf = svgToPdf;

ipcMain.handle('export-pdf', async (e, { svg, width, height, defaultName }) => {
  const r = await dialog.showSaveDialog(winOf(e), { defaultPath: defaultName, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
  if (r.canceled || !r.filePath) return null;
  fs.writeFileSync(r.filePath, await svgToPdf(svg, width, height));
  return { path: r.filePath };
});
ipcMain.on('open-external', (e, url) => { if (typeof url === 'string' && /^(https?:|mailto:)/i.test(url)) shell.openExternal(url); });
ipcMain.on('set-state', (e, s) => {
  const win = winOf(e);
  const st = win && state.get(win);
  if (!st) return;
  st.dirty = !!s.dirty; st.path = s.path || null; st.title = s.title || 'Mindloom';
  win.setTitle(st.title);
  if (isMac) { win.setDocumentEdited(st.dirty); if (st.path) win.setRepresentedFilename(st.path); }
});
ipcMain.on('force-close', (e) => { const win = winOf(e); const st = win && state.get(win); if (st) { st.forceClose = true; win.close(); } });
ipcMain.handle('set-fullscreen', (e, on) => { const w = winOf(e); if (w) w.setFullScreen(!!on); });
ipcMain.handle('get-recent', () => getRecent().filter((p) => fs.existsSync(p)));

// ---------------- AI assistant (user's own Anthropic API key) ----------------
const keyFile = () => path.join(app.getPath('userData'), 'ai-key.bin');
function loadKey() {
  try {
    const buf = fs.readFileSync(keyFile());
    return safeStorage.isEncryptionAvailable() ? safeStorage.decryptString(buf) : null;
  } catch (e) { return null; }
}
ipcMain.handle('ai-key', (e, op, key) => {
  if (op === 'has') return !!loadKey() || !!process.env.ANTHROPIC_API_KEY;
  if (op === 'clear') { try { fs.unlinkSync(keyFile()); } catch (err) { /* ignore */ } return true; }
  if (op === 'set' && typeof key === 'string' && key.trim()) {
    if (!safeStorage.isEncryptionAvailable()) throw new Error('Secure storage is not available on this system.');
    fs.writeFileSync(keyFile(), safeStorage.encryptString(key.trim()));
    return true;
  }
  return false;
});

const AI_SYSTEM = 'You are the brainstorming engine of a mind-mapping app. Reply with ONLY an indented outline: one topic per line, indented with two spaces per level. No bullets, numbering, markdown, quotes, emojis or commentary. Keep every topic short (at most 7 words) and specific.';

ipcMain.handle('ai', async (e, req) => {
  const apiKey = loadKey() || process.env.ANTHROPIC_API_KEY;
  if (!apiKey) throw new Error('Add your Anthropic API key first.');
  const mod = require('@anthropic-ai/sdk');
  const Anthropic = mod.default || mod;
  const client = new Anthropic({ apiKey, timeout: 120000, maxRetries: 2 });
  const clip = (s, n) => String(s || '').slice(0, n);
  let prompt;
  if (req && req.mode === 'expand') {
    prompt = `Here is the current mind map as an indented outline:\n<map>\n${clip(req.outline, 6000)}\n</map>\n\nSuggest 4 to 7 NEW subtopics for the topic at this path: ${(req.path || []).map((x) => clip(x, 120)).join(' > ')}.\nDo not repeat existing subtopics: ${(req.existing || []).map((x) => clip(x, 80)).join('; ') || 'none'}.\nReply with the new subtopics only (no line for the parent). You may add up to 3 indented children under each.`;
  } else {
    prompt = `Create a rich mind map about: ${clip(req && req.topic, 500)}\nThe first line is the central topic. Give 5 to 7 main branches, each with 2 to 5 subtopics; go one level deeper where it adds real value.`;
  }
  let response;
  try {
    response = await client.beta.messages.create({
      model: 'claude-opus-5-5',
      max_tokens: 16000,
      betas: ['server-side-fallback-2026-07-01'],
      fallbacks: 'default',
      output_config: { effort: 'medium' },
      system: AI_SYSTEM,
      messages: [{ role: 'user', content: prompt }],
    });
  } catch (err) {
    if (err instanceof Anthropic.AuthenticationError) throw new Error('Your API key was rejected. Check it and try again.');
    if (err instanceof Anthropic.PermissionDeniedError) throw new Error('This API key is not allowed to use the model.');
    if (err instanceof Anthropic.RateLimitError) throw new Error('Rate limited by the API. Please wait a moment and retry.');
    if (err instanceof Anthropic.APIConnectionError) throw new Error('Could not reach api.anthropic.com. Check your internet connection.');
    if (err instanceof Anthropic.APIError) throw new Error(`API error ${err.status || ''}: ${err.message}`);
    throw err;
  }
  if (response.stop_reason === 'refusal') throw new Error('The model declined this request. Try rephrasing the topic.');
  const text = response.content.filter((b) => b.type === 'text').map((b) => b.text).join('\n');
  const outline = text.replace(/```[a-z]*\n?|```/gi, '').split('\n').filter((l) => l.trim()).join('\n');
  if (!outline.trim()) throw new Error('The model returned an empty answer.');
  return { outline };
});

// ---------------- app lifecycle ----------------
if (!process.env.MINDLOOM_SMOKE && !app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', (e, argv) => {
    const p = fileArg(argv);
    if (p) openInWindow(p);
    else { const w = focused(); if (w) { if (w.isMinimized()) w.restore(); w.focus(); } }
  });
  app.on('open-file', (e, p) => { e.preventDefault(); if (app.isReady()) openInWindow(p); else pendingOpen.push(p); });
  app.whenReady().then(() => {
    buildMenu();
    const initial = pendingOpen.shift() || fileArg(process.argv);
    const win = createWindow(initial);
    pendingOpen.forEach((p) => setTimeout(() => win.webContents.send('open-path', p), 1500));
    pendingOpen = [];
    if (process.env.MINDLOOM_SMOKE) require('../../test/smoke-electron').run(win, { svgToPdf });
    app.on('activate', () => { if (!windows.size) createWindow(); });
  });
  app.on('window-all-closed', () => { if (!isMac || process.env.MINDLOOM_SMOKE) app.quit(); });
}
