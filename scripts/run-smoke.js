'use strict';
// Launches the real Electron app in self-test mode. Usage: node scripts/run-smoke.js [outputDir] [path/to/packaged/binary]
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');

const out = path.resolve(process.argv[2] || fs.mkdtempSync(path.join(os.tmpdir(), 'mindloom-smoke-')));
fs.mkdirSync(out, { recursive: true });
const packaged = process.argv[3];
const bin = packaged || require('electron');
const args = packaged ? [] : [path.join(__dirname, '..')];
const r = spawnSync(bin, args, { stdio: 'inherit', env: Object.assign({}, process.env, { MINDLOOM_SMOKE: out }), timeout: 180000 });
console.log(`Smoke output: ${out}`);
process.exit(r.status == null ? 1 : r.status);
