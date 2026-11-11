/* Mindloom — startup */
(function (ML) {
  'use strict';
  const App = ML.app, P = ML.platform, M = ML.model;

  function welcomeDoc() {
    const d = ML.io.fromText(`Welcome to Mindloom
  Create
    Tab adds a subtopic
    Enter adds a topic
    Just start typing to rename
    Double-click the canvas for a floating topic
  Organise
    Drag topics anywhere, they stay connected
    Drop onto a topic to re-parent
    Collapse branches with the ⊖ handle
    Focus on one branch
  Style
    16 colour themes
    9 structures
    Shapes, lines and backgrounds
    Rainbow branches
  Enrich
    Markers and stickers
    Notes and links
    Images
    Relationships and boundaries
  Share
    PNG, SVG and PDF
    Markdown, OPML, FreeMind
    Opens XMind files
    Presentation mode`);
    const c = d.root.children;
    c[0].icons = ['p1']; c[1].icons = ['🧠']; c[2].icons = ['🎨'];
    c[3].icons = ['💡']; c[4].icons = ['🚀'];
    c[3].children[1].note = 'Notes appear in the Notes tab on the right. Hover the note icon to preview them.';
    c[3].children[1].link = 'https://github.com/';
    c[1].children[2].task = 'done';
    c[1].children[3].task = 'todo';
    c[4].children[0].tags = ['export'];
    c[2].boundary = true;
    c[2].boundaryLabel = 'Make it yours';
    M.addRelation(d, c[0].id, c[4].id, 'then');
    d.settings.theme = 'fresh';
    d.settings.lineStyle = 'tapered';
    return d;
  }
  ML.welcomeDoc = welcomeDoc;

  async function start() {
    App.init();
    App.initPanels();
    const bridge = window.mindloom;
    const initial = bridge && bridge.getInitialPath ? await bridge.getInitialPath() : null;
    if (initial) {
      App.setDoc(welcomeDoc());
      const r = await P.readPath(initial);
      if (r && !r.error) { await App.loadFile(r); return; }
    }
    if (bridge) {
      // desktop: recover a crashed session, or reopen the file that was open last time
      const sess = await P.restoreSession().catch(() => null);
      if (sess && sess.kind === 'autosave' && sess.data && sess.data.doc) {
        const d = sess.data;
        if (!d.dirty && d.path) {
          const r = await P.readPath(d.path);
          if (r && !r.error && (await App.loadFile(r))) return;
        }
        try {
          App.setDoc(d.doc, { path: d.path, name: d.name, dirty: true });
          App.toast('Recovered your unsaved changes from the last session.', 4000);
          return;
        } catch (e) { console.warn('Could not restore autosave', e); }
      } else if (sess && sess.kind === 'path') {
        const r = await P.readPath(sess.path);
        if (r && !r.error && (await App.loadFile(r))) return;
      }
    } else {
      const saved = P.store.get('autosave');
      if (saved && saved.doc) {
        try { App.setDoc(saved.doc, { path: saved.path, name: saved.name, dirty: saved.dirty }); return; } catch (e) { console.warn('Could not restore autosave', e); }
      }
    }
    if (bridge && bridge.isFirstWindow && !bridge.isFirstWindow()) {
      App.setDoc(ML.templates.build('blank'), { name: 'Untitled' });
      setTimeout(() => App.startEdit(App.doc.root.id), 400);
      return;
    }
    App.setDoc(welcomeDoc(), { name: 'Welcome' });
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})(window.ML = window.ML || {});
