/* Mindloom — platform adapter: Electron (via preload bridge) or plain browser fallbacks */
(function (ML) {
  'use strict';
  const bridge = window.mindloom || null;

  function download(name, data, mime) {
    const blob = data instanceof Blob ? data : new Blob([data], { type: mime || 'application/octet-stream' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = name;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
  }

  function pickFile(accept) {
    return new Promise((resolve) => {
      const inp = document.createElement('input');
      inp.type = 'file';
      inp.accept = accept || '';
      inp.onchange = () => resolve(inp.files[0] || null);
      inp.click();
    });
  }

  const b64ToBuf = (b64) => { const bin = atob(b64); const u = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i); return u.buffer; };
  const bufToB64 = async (blob) => {
    const buf = new Uint8Array(await blob.arrayBuffer());
    let s = '';
    for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode.apply(null, buf.subarray(i, i + 0x8000));
    return btoa(s);
  };

  // errors thrown in the main process arrive as "Error invoking remote method 'x': Error: message"
  const cleanErr = (e) => { throw new Error(String((e && e.message) || e).replace(/^Error invoking remote method '[^']+':\s*(Error:\s*)?/, '')); };

  const OPEN_EXT = ['mindloom', 'json', 'md', 'markdown', 'txt', 'opml', 'mm', 'xmind'];

  const P = {
    isElectron: !!bridge,
    os: bridge ? bridge.platform : (/Mac/.test(navigator.platform) ? 'darwin' : /Win/.test(navigator.platform) ? 'win32' : 'linux'),

    /** Returns {name, path|null, text?|buffer?} or null */
    async open() {
      if (bridge) {
        const r = await bridge.openFile();
        if (!r) return null;
        return Object.assign(r, r.base64 ? { buffer: b64ToBuf(r.base64) } : {});
      }
      const f = await pickFile(OPEN_EXT.map((e) => '.' + e).join(','));
      if (!f) return null;
      if (/\.xmind$/i.test(f.name)) return { name: f.name, path: null, buffer: await f.arrayBuffer() };
      return { name: f.name, path: null, text: await f.text() };
    },

    async readPath(path) {
      if (!bridge) return null;
      const r = await bridge.readPath(path);
      return r && r.base64 ? Object.assign(r, { buffer: b64ToBuf(r.base64) }) : r;
    },

    /** Save native document. Returns {path, name} or null if cancelled. */
    async save(text, path, suggestedName) {
      if (bridge) return bridge.saveFile({ path, text, defaultName: suggestedName });
      download(suggestedName, text, 'application/json');
      return { path: null, name: suggestedName };
    },

    /** Export text or Blob with a save dialog. */
    async exportFile(name, data, ext, label) {
      if (bridge) {
        const isBlob = data instanceof Blob;
        return bridge.exportFile({ defaultName: name, ext, label, base64: isBlob ? await bufToB64(data) : null, text: isBlob ? null : data });
      }
      download(name, data);
      return { path: name };
    },

    async exportPDF(svg, w, h, name) {
      if (bridge) return bridge.exportPDF({ svg, width: w, height: h, defaultName: name });
      const win = window.open('', '_blank');
      if (!win) return null;
      win.document.write(`<!doctype html><title>${name}</title><style>@page{size:${w}px ${h}px;margin:0}body{margin:0}</style>${svg}<script>onload=()=>{print()}<\/script>`);
      win.document.close();
      return { path: null };
    },

    openExternal(url) {
      if (!/^(https?:|mailto:)/i.test(url)) return;
      if (bridge) bridge.openExternal(url); else window.open(url, '_blank', 'noopener');
    },

    setTitle(title, dirty, path) { document.title = title; if (bridge) bridge.setState({ title, dirty, path }); },
    onMenu(cb) { if (bridge) bridge.onMenu(cb); },
    onOpenPath(cb) { if (bridge) bridge.onOpenPath(cb); },
    fullscreen(on) {
      if (bridge) return bridge.setFullScreen(on);
      if (on && document.documentElement.requestFullscreen) document.documentElement.requestFullscreen().catch(() => {});
      else if (!on && document.fullscreenElement) document.exitFullscreen().catch(() => {});
    },
    recent() { return bridge ? bridge.getRecent() : Promise.resolve([]); },
    ai(req) { return bridge ? bridge.ai(req).catch(cleanErr) : Promise.reject(new Error('The AI assistant is available in the desktop app.')); },
    aiConfig() { return bridge ? bridge.aiConfig() : Promise.resolve(null); },
    aiSetConfig(patch) { return bridge ? bridge.aiSetConfig(patch).catch(cleanErr) : Promise.resolve(false); },
    aiSetKey(provider, key) { return bridge ? bridge.aiSetKey(provider, key).catch(cleanErr) : Promise.resolve(false); },
    aiClearKey(provider) { return bridge ? bridge.aiClearKey(provider) : Promise.resolve(false); },
    aiModels(provider) { return bridge ? bridge.aiModels(provider).catch(cleanErr) : Promise.resolve([]); },
    /** Crash-safe autosave: a file in the desktop app, browser storage on the web. */
    autosave(data) {
      if (bridge && bridge.autosave) return bridge.autosave(JSON.stringify(data)).then(() => true, () => false);
      return Promise.resolve(P.store.set('autosave', data));
    },
    restoreSession() { return bridge && bridge.restoreSession ? bridge.restoreSession() : Promise.resolve(null); },
    pickFile, download,
    store: {
      get(k) { try { return JSON.parse(localStorage.getItem('mindloom:' + k)); } catch (e) { return null; } },
      set(k, v) { try { localStorage.setItem('mindloom:' + k, JSON.stringify(v)); return true; } catch (e) { return false; } },
    },
  };
  ML.platform = P;
})(window.ML = window.ML || {});
