'use strict';
// Renders build/icon.svg to build/icon.png (1024px, transparent). Run: npx electron scripts/make-icon.js
const { app, BrowserWindow } = require('electron');
const fs = require('node:fs');
const path = require('node:path');

app.disableHardwareAcceleration();
app.whenReady().then(async () => {
  const dir = path.join(__dirname, '..', 'build');
  const svg = fs.readFileSync(path.join(dir, 'icon.svg'), 'utf8');
  const win = new BrowserWindow({ width: 1024, height: 1024, show: false, frame: false, transparent: true, useContentSize: true, webPreferences: { offscreen: true } });
  await win.loadURL('data:text/html;charset=utf-8,' + encodeURIComponent(`<style>html,body{margin:0;background:transparent;overflow:hidden}svg{display:block}</style>${svg}`));
  await new Promise((r) => setTimeout(r, 400));
  const img = await win.webContents.capturePage({ x: 0, y: 0, width: 1024, height: 1024 });
  fs.writeFileSync(path.join(dir, 'icon.png'), img.resize({ width: 1024, height: 1024 }).toPNG());
  console.log('icon.png written', img.getSize());
  app.quit();
});
