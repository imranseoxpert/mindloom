'use strict';
/* Mindloom — preload bridge (sandboxed): exposes a narrow, typed API to the page */
const { contextBridge, ipcRenderer, webUtils } = require('electron');

const arg = (name) => { const a = process.argv.find((x) => x.startsWith(`--${name}=`)); return a ? a.slice(name.length + 3) : null; };

contextBridge.exposeInMainWorld('mindloom', {
  platform: process.platform,
  version: arg('ml-version'),
  isFirstWindow: () => arg('ml-first') === '1',
  getInitialPath: () => ipcRenderer.invoke('get-initial-path'),
  openFile: () => ipcRenderer.invoke('open-file'),
  readPath: (p) => ipcRenderer.invoke('read-path', String(p)),
  saveFile: (o) => ipcRenderer.invoke('save-file', o),
  exportFile: (o) => ipcRenderer.invoke('export-file', o),
  exportPDF: (o) => ipcRenderer.invoke('export-pdf', o),
  openExternal: (url) => ipcRenderer.send('open-external', String(url)),
  setState: (s) => ipcRenderer.send('set-state', s),
  forceClose: () => ipcRenderer.send('force-close'),
  setFullScreen: (on) => ipcRenderer.invoke('set-fullscreen', !!on),
  getRecent: () => ipcRenderer.invoke('get-recent'),
  ai: (req) => ipcRenderer.invoke('ai', req),
  aiConfig: () => ipcRenderer.invoke('ai-config'),
  aiSetConfig: (patch) => ipcRenderer.invoke('ai-config-set', patch),
  aiSetKey: (provider, key) => ipcRenderer.invoke('ai-key-set', String(provider), String(key)),
  aiClearKey: (provider) => ipcRenderer.invoke('ai-key-clear', String(provider)),
  aiModels: (provider) => ipcRenderer.invoke('ai-models', String(provider)),
  autosave: (json) => ipcRenderer.invoke('autosave-write', String(json)),
  restoreSession: () => ipcRenderer.invoke('session-restore'),
  pathForFile: (file) => { try { return webUtils.getPathForFile(file); } catch (e) { return null; } },
  onMenu: (cb) => ipcRenderer.on('menu', (e, cmd, a) => cb(cmd, a)),
  onOpenPath: (cb) => ipcRenderer.on('open-path', (e, p) => cb(p)),
});
