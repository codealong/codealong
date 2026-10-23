// Checks paths an agent writes as code in a reply: a full path, or one under home, that's on this
// Mac reads as it's written, underlined, and opens on a click, in your editor at its line where
// it gives one; a right-click offers Open, your editor, the Finder and its path, in place of the
// message's own menu. A name with a dash, an ampersand or a quote in it is a path like any other;
// what only looks like one (a route, a skill, a command, a file that's gone) stays code, and every
// path in the reply is asked about in one go. A file Codex cites has
// the same menu. The conversation is a stand-in, on the first session:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/path-links-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=path-links) it leaves the panel open on it, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var STEPS = '/private/tmp/claude-501/-Users-me-projects-northwind/118c2b2f/scratchpad/export-steps.md';
  var MAIN = '~/projects/northwind/src/main.rs';
  var FOLDER = '/Users/me/projects/northwind/docs/';
  var CLEARED = '/private/tmp/claude-501/-Users-me-projects-northwind/118c2b2f/scratchpad/cleared.md';
  var CITED = '/Users/me/projects/northwind/README.md';
  var PLAN = '/Users/me/projects/northwind/docs/Plan - final.md';
  // What the engine finds on this stand-in Mac: these, and nothing else.
  var HERE = {};
  HERE[STEPS] = 'file';
  HERE[MAIN] = 'file';
  HERE[FOLDER] = 'folder';
  HERE[CLEARED] = 'file';
  HERE[PLAN] = 'file';
  var REPLY = [
    '1. **Workspace exports.** These are the 8 steps in `' + STEPS + '`. You said you’d do them this week.',
    '2. **The fix** is at `' + MAIN + ':42`, and the notes are in `' + FOLDER + '`, the plan in `' + PLAN + '`.',
    '3. Not paths: `/api/v1/users`, `/release`, `/usr/bin/open -R ' + STEPS + '` and `/Users/me/projects/northwind/gone.md`.',
    '4. The old plan was `' + CLEARED + '`. See :codex-file-citation{path="' + CITED + '"}.',
  ].join('\n');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n) { return { at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }; };
      return Promise.resolve({ before: null, after: 2, items: [item('you', 'What’s left for me to do?', 0), item('agent', REPLY, 1)] });
    }
    if (cmd === 'path_kinds') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(args.paths.map(function (p) { return HERE[p] || null; }));
    }
    // A file that was there when the reply showed and was cleared before the click.
    if (cmd === 'open_file' && /cleared/.test((args || {}).path)) {
      window.__calls.push([cmd, args, options]);
      return Promise.reject('cleared.md isn’t on this Mac any more.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var reply = function () { return all('.ca-sessinsp .ca-thread-said.is-agent').filter(function (m) { return /Workspace exports/.test(m.textContent); })[0]; };
  var links = function () { return all('.ca-ic-path', reply()); };
  var linkTo = function (text) { return links().filter(function (c) { return c.textContent === text; })[0]; };

  say('path links check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return reply() && links().length >= 5; }, 10000);
    })
    .then(function () {
      var shown = links().map(function (c) { return c.textContent; });
      check('paths on this Mac are links, as written', shown.join('|') === [STEPS, MAIN + ':42', FOLDER, PLAN, CLEARED].join('|'), shown.join('|'));
      var plain = all('code.ca-ic', reply()).filter(function (c) { return !c.classList.contains('ca-ic-path'); }).map(function (c) { return c.textContent; });
      check('what only looks like a path stays code', plain.join('|') === ['/api/v1/users', '/release', '/usr/bin/open -R ' + STEPS, '/Users/me/projects/northwind/gone.md'].join('|'), plain.join('|'));
      var asked = sent('path_kinds');
      check('the reply’s paths are asked about in one go', asked.length === 1 && asked[0].paths.length === 8, JSON.stringify(asked));
      check('a skill is never asked about', asked.length === 1 && asked[0].paths.indexOf('/release') < 0, JSON.stringify(asked));
      check('underlined as a link is', getComputedStyle(linkTo(STEPS)).textDecorationLine === 'underline');

      linkTo(STEPS).click();
      check('a click opens the file', (last('open_file') || {}).path === STEPS, JSON.stringify(last('open_file')));
      linkTo(MAIN + ':42').click();
      check('one with a line opens in your editor there', JSON.stringify(last('edit_file')) === JSON.stringify({ path: MAIN, line: 42 }), JSON.stringify(last('edit_file')));
      linkTo(FOLDER).click();
      check('a folder opens as the Finder shows it', (last('open_file') || {}).path === FOLDER, JSON.stringify(last('open_file')));
      linkTo(CLEARED).click();
      return until(function () { return q('.ca-md-file-why', reply()); }, 3000);
    })
    .then(function (why) {
      check('one that’s gone since says so in place', why.textContent === 'cleared.md isn’t on this Mac any more.', why.textContent);
      return menuOf(linkTo(STEPS));
    })
    .then(function (labels) {
      check('a right-click offers its own menu, not the message’s', labels.join('|') === 'Open|Open in Cursor|Reveal in Finder|Copy path', labels.join('|'));
      menuItem('Reveal in Finder')();
      check('Reveal in Finder shows it', (last('reveal_file') || {}).path === STEPS, JSON.stringify(last('reveal_file')));
      menuItem('Open in Cursor')();
      check('Open in Cursor opens it there', JSON.stringify(last('edit_file')) === JSON.stringify({ path: STEPS, line: null }), JSON.stringify(last('edit_file')));
      return menuOf(linkTo(FOLDER));
    })
    .then(function (labels) {
      check('a folder’s has no Open', labels.indexOf('Open') < 0 && labels.indexOf('Open in Cursor') >= 0, labels.join('|'));
      return menuOf(q('button.ca-md-file', reply()));
    })
    .then(function (labels) {
      check('a file Codex cites has the same menu', labels.indexOf('Open') >= 0 && labels.indexOf('Reveal in Finder') >= 0, labels.join('|'));
    })
    .then(done)
    .catch(stopped);
})();
