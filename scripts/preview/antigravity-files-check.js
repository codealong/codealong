// Checks Antigravity-style file presentation in a conversation: a user-facing artifact write_file
// stays unfolded with its summary and opens in the in-app Markdown preview dialog (including its
// interactive carousel), `[label](file:///…)` links open the preview for Markdown files or jump to
// `#L` lines in your editor while missing ones stay plain text, and local Markdown images in
// macOS-protected folders gate behind a Show image button.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/antigravity-files-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var MD = '/Users/me/.gemini/antigravity-cli/brain/dd6aac2d/tool-rendering-mocks.md';
  var CODE = '/Users/me/projects/codealong/src/ui/Markdown.tsx';
  var GONE = '/Users/me/.gemini/antigravity-cli/brain/dd6aac2d/gone.md';
  var DESKTOP_IMG = '/Users/me/Desktop/mock-shot.png';
  var CAPTION = 'Visual mocks comparing Current, Compact, Folded, and Zen modes.';
  var DOC = [
    '# Session Readability — Mocks',
    '',
    'Side-by-side comparison using Codealong’s design tokens:',
    '',
    '````carousel',
    '### Option 1 · Compact',
    '',
    '```ts',
    'const summary = "Ran 5 commands";',
    '```',
    '<!-- slide -->',
    '### Option 2 · Folded',
    '',
    '```ts',
    'const summary = "Worked for 2m · 19 tool calls";',
    '```',
    '````',
    '',
    '| Mode | Tool runs | Always visible |',
    '| --- | --- | --- |',
    '| **Compact** | 1-line summary | Failed calls, artifacts |',
    '| **Folded** | Collapses completed turns | Active turns, questions |',
  ].join('\n');

  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 4 * 60e3;
      var item = function (kind, text, n, extra) { return Object.assign({ at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({
        before: null,
        after: 6,
        items: [
          item('you', 'Can you show me some mocks first?', 0),
          item('tool', 'Write file · `tool-rendering-mocks.md`', 1, {
            id: 't1',
            file: MD,
            input: JSON.stringify({ Path: MD }, null, 2),
            sent: { files: [MD], caption: CAPTION },
          }),
          item('result', '', 2, { id: 't1', output: 'Created file.' }),
          item(
            'agent',
            [
              'I put together mocks in [tool-rendering-mocks.md](file://' + MD + ') (see [Markdown.tsx](file://' + CODE + '#L344-L359), not [gone.md](file://' + GONE + ')).',
              '',
              '![Desktop mock](' + DESKTOP_IMG + ')',
            ].join('\n'),
            3,
          ),
        ],
      });
    }
    if (cmd === 'path_kinds') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve((args.paths || []).map(function (p) { return p === MD || p === CODE ? 'file' : null; }));
    }
    if (cmd === 'sent_files') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve((args.paths || []).map(function (p) { return p === DESKTOP_IMG ? 'asksFirst' : 'gone'; }));
    }
    if (cmd === 'read_markdown') {
      window.__calls.push([cmd, args, options]);
      if ((args || {}).path === MD) return Promise.resolve({ kind: 'text', text: DOC });
      return Promise.reject('Not found');
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
  var panel = function (sel) { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp ' + sel)); };

  say('antigravity files check: opening conversation');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return panel('.ca-thread-sent').length > 0 && panel('.ca-thread-agent a.ca-md-a').length === 2; }, 10000);
    })
    .then(function () {
      var sent = panel('.ca-thread-sent')[0];
      check('artifact summary shows under its write_file call', sent.querySelector('.ca-thread-sentcaption').textContent === CAPTION);
      var mdBtn = sent.querySelector('button.ca-md-file');
      check('sent Markdown artifact advertises Preview', !!mdBtn && mdBtn.title === 'Preview ' + MD, mdBtn ? mdBtn.title : 'none');

      var replyLinks = panel('.ca-thread-agent a.ca-md-a');
      check('existing file:// links render clickable and missing ones stay plain text', replyLinks.map(function (a) { return a.textContent; }).join('|') === 'tool-rendering-mocks.md|Markdown.tsx', replyLinks.map(function (a) { return a.textContent; }).join('|'));

      var askBtn = panel('.ca-thread-agent .ca-md-img-ask button')[0];
      check('protected local Markdown image waits behind Show image', !!askBtn && /Show image/.test(askBtn.textContent), askBtn ? askBtn.textContent : 'none');

      // Line-anchored file:// link opens in editor at line 344.
      replyLinks[1].click();
      return until(function () { return window.__calls.some(function (c) { return c[0] === 'edit_file'; }); }, 3000);
    })
    .then(function () {
      var edit = window.__calls.filter(function (c) { return c[0] === 'edit_file'; }).pop();
      check('line-anchored file:// link opens in editor at line', edit && edit[1].path === CODE && edit[1].line === 344, JSON.stringify(edit));

      // Right-clicking the file:// Markdown link offers Preview first.
      return menuOf(panel('.ca-thread-agent a.ca-md-a')[0]);
    })
    .then(function (labels) {
      check('file:// Markdown context menu offers Preview', labels.join('|') === 'Preview|Open|Open in Cursor|Reveal in Finder|Copy path', labels.join('|'));

      // Clicking the sent Markdown artifact opens the preview dialog with its carousel.
      panel('.ca-thread-sent button.ca-md-file')[0].click();
      return until(function () { return q('.ca-dialog.ca-mdpreview .ca-md-carousel'); }, 5000);
    })
    .then(function () {
      var dlg = q('.ca-dialog.ca-mdpreview');
      check('preview dialog shows filename and rendered carousel slide 1', dlg.querySelector('h2').textContent === 'tool-rendering-mocks.md' && /Slide 1 of 2/.test(dlg.querySelector('.ca-md-carousel-head').textContent) && /Option 1 · Compact/.test(dlg.textContent));
      dlg.querySelector('button[aria-label="Next slide"]').click();
      return until(function () { return /Slide 2 of 2/.test(dlg.querySelector('.ca-md-carousel-head').textContent); }, 3000);
    })
    .then(function () {
      var dlg = q('.ca-dialog.ca-mdpreview');
      check('carousel advances to slide 2', /Option 2 · Folded/.test(dlg.textContent));
    })
    .then(done)
    .catch(stopped);
})();
