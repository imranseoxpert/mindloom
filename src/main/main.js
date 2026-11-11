'use strict';
/* Mindloom — Electron main process: windows, menus, file dialogs, PDF export, AI assistant */
const { app, BrowserWindow, Menu, dialog, ipcMain, shell, safeStorage, nativeTheme } = require('electron');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');

const isMac = process.platform === 'darwin';
const OPEN_EXT = ['mindloom', 'json', 'md', 'markdown', 'txt', 'opml', 'mm', 'xmind', 'csv'];
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
  state.set(win, { dirty: false, path: null, title: 'Mindloom', forceClose: false, initialPath: initialPath || null, isFirst, autosaveKey: `${Date.now().toString(36)}-${win.id}` });
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
      win.close();
    }
  });
  // closed cleanly (saved, unchanged, or "Don't Save"): forget the crash copy and remember the file for next launch
  win.on('closed', () => {
    const st = state.get(win);
    clearAutosave(win);
    if (st && st.path && !process.env.MINDLOOM_SMOKE) { try { fs.writeFileSync(lastFile(), JSON.stringify({ path: st.path })); } catch (e) { /* ignore */ } }
    windows.delete(win);
  });
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
      c('Import (Markdown, OPML, XMind, FreeMind, Text, CSV)…', 'open'),
      { label: 'Export', submenu: [
        c('PNG Image…', 'exportPNG'), c('SVG Vector…', 'exportSVG'), c('PDF Document…', 'exportPDF'), { type: 'separator' },
        c('Web Page (.html)…', 'exportHTML'),
        c('Markdown…', 'exportMarkdown'), c('OPML…', 'exportOPML'), c('Plain Text Outline…', 'exportText'), c('FreeMind (.mm)…', 'exportFreeMind'), c('Spreadsheet (.csv)…', 'exportCSV'),
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
      c('Set Due Date…', 'due'),
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

// ---------------- AI assistant: bring your own key (any provider) ----------------
require('./ai').register({ ipcMain, app, safeStorage, smoke: !!process.env.MINDLOOM_SMOKE });

// ---------------- crash-safe autosave + resume last session ----------------
// Each window writes its unsaved state to userData/autosave/<key>.json. A clean close deletes it, so a file
// left behind means the app crashed or was killed: it is offered back on the next start.
const autosaveDir = () => path.join(app.getPath('userData'), 'autosave');
const lastFile = () => path.join(app.getPath('userData'), 'last-session.json');
const safeKey = (k) => String(k || '').replace(/[^a-z0-9-]/gi, '').slice(0, 60);
ipcMain.handle('autosave-write', (e, json) => {
  const st = state.get(winOf(e));
  if (!st || typeof json !== 'string') return false;
  fs.mkdirSync(autosaveDir(), { recursive: true });
  const f = path.join(autosaveDir(), safeKey(st.autosaveKey) + '.json');
  fs.writeFileSync(f + '.tmp', json, 'utf8');
  fs.renameSync(f + '.tmp', f);
  return true;
});
function clearAutosave(win) {
  const st = state.get(win);
  if (!st) return;
  try { fs.unlinkSync(path.join(autosaveDir(), safeKey(st.autosaveKey) + '.json')); } catch (e) { /* none */ }
}
/** For the first window: a crashed session's autosave, else the file that was open last time. */
ipcMain.handle('session-restore', (e) => {
  const st = state.get(winOf(e));
  if (!st || !st.isFirst) return null;
  try {
    const own = safeKey(st.autosaveKey) + '.json';
    const files = fs.readdirSync(autosaveDir()).filter((f) => f.endsWith('.json') && f !== own)
      .map((f) => ({ f, t: fs.statSync(path.join(autosaveDir(), f)).mtimeMs })).sort((a, b) => b.t - a.t);
    if (files.length) {
      const text = fs.readFileSync(path.join(autosaveDir(), files[0].f), 'utf8');
      // the restored copy now belongs to this window; drop the old file and any older than a week
      for (const x of files) if (x === files[0] || Date.now() - x.t > 7 * 864e5) fs.rmSync(path.join(autosaveDir(), x.f), { force: true });
      return { kind: 'autosave', data: JSON.parse(text) };
    }
  } catch (err) { /* no autosaves */ }
  try {
    const last = JSON.parse(fs.readFileSync(lastFile(), 'utf8'));
    if (last && typeof last.path === 'string' && fs.existsSync(last.path)) return { kind: 'path', path: last.path };
  } catch (err) { /* first run */ }
  return null;
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
