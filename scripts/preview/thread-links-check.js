// Checks the links in a session's conversation that aren't in a reply: a call that made a page on
// the web (an artifact Claude Code published) says so under its line, as a link that opens it,
// and stays in view while the calls around it fold; web links in a tool's output open too, with
// the output's text as it was; and a call that sent you files lists them under its line with its
// caption, each opening on a click, and one that's gone says so in place; a call that read a file
// names it in its line, the name opening it and the line offering its menu. The conversation is a
// stand-in, on the first session:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/thread-links-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=thread-links) it leaves the panel open on it, the publish's
// output showing, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var URL = 'https://claude.ai/artifact/ExampleArtifactId00001';
  var FILE = '/private/tmp/claude-501/-Users-me-projects-halyard/20898bb3/scratchpad/security-dashboard.html';
  var PUBLISHED = 'Published ' + FILE + ' at ' + URL + ' (Version 9, version id 1790960406-8f4f)\n\nLive subscription: already connected from earlier in this session.\n\nTo update: publish the same file path here (keeps this URL).';
  var SENT = ['/Users/me/projects/halyard/tmp/dashboard.png', '/private/tmp/claude-501/-Users-me-projects-halyard/20898bb3/scratchpad/gone/before.png'];
  var CAPTION = 'The refreshed dashboard, and the one it replaces';
  var READ = '/Users/me/projects/halyard/config/slots.json';
  var CONFIG = '/Users/me/projects/halyard/config.toml';
  var SUMMARY = 'Reading the configuration before updating the development environment';
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n, extra) { return Object.assign({ at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({ before: null, after: 20, items: [
        item('you', 'Refresh the security dashboard and publish it', 0),
        item('agent', 'The verification build is running now. I’ll publish the refreshed dashboard when it finishes.', 1),
        item('tool', 'Bash · `npm run verify`', 2, { id: 't1', input: 'npm run verify' }),
        item('result', '', 3, { id: 't1', output: 'ok' }),
        item('tool', 'Read · `slots.json`', 4, { id: 't2', input: READ, file: READ }),
        item('result', '', 5, { id: 't2', output: '{"docs":"https://example.com/a_b","next":"https://example.com/c"}' }),
        item('tool', 'Bash · `tail -12 build.log`', 6, { id: 't3', input: 'tail -12 build.log' }),
        item('result', '', 7, { id: 't3', output: 'done' }),
        item('tool', 'Artifact · `security-dashboard.html`', 8, { id: 't4', input: JSON.stringify({ file_path: FILE, label: 'Halyard current 2 Oct' }, null, 2) }),
        item('result', '', 9, { id: 't4', output: PUBLISHED, link: { url: URL, title: 'Halyard security dashboard', note: 'Updated · version 9' } }),
        item('tool', 'Bash · `git status --short`', 10, { id: 't5', input: 'git status --short' }),
        item('result', '', 11, { id: 't5', output: '' }),
        item('tool', 'SendUserFile · `dashboard.png` and 1 more', 12, { id: 't6', input: JSON.stringify({ files: SENT, caption: CAPTION, status: 'normal', display: 'render' }, null, 2), sent: { files: SENT, caption: CAPTION } }),
        item('result', '', 13, { id: 't6', output: '2 files delivered to user.' }),
        item('agent', 'Everything checks out, and the dashboard is published.', 14),
        // A line that says what the call was for (Antigravity's) has the file after it.
        item('tool', 'View file · ' + SUMMARY, 15, { id: 't7', input: CONFIG, file: CONFIG }),
        item('result', '', 16, { id: 't7', output: '[server]' }),
      ] });
    }
    // A sent file in a scratch folder that's been cleared since.
    if (cmd === 'open_file' && /gone/.test((args || {}).path)) {
      window.__calls.push([cmd, args, options]);
      return Promise.reject('before.png isn’t on this machine any more.');
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
  var opened = function () { return window.__calls.filter(function (c) { return c[0] === 'open_url'; }); };

  say('thread links check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return panel('.ca-thread-made').length > 0; }, 10000);
    })
    .then(function () {
      var made = panel('.ca-thread-made');
      check('the publish says what it made', made.length === 1 && made[0].querySelector('a').textContent.trim() === 'Halyard security dashboard', made.map(function (m) { return m.textContent; }).join('|'));
      check('and what it did with it', /Updated · version 9/.test(made[0].querySelector('.ca-meta').textContent));
      var tool = made[0].closest('.ca-thread-tool');
      check('under its own call’s line', /^Artifact · security-dashboard\.html/.test(tool.querySelector('.ca-thread-toollabel').textContent));
      // Three calls ahead of it fold into a line and the latest of them; it and the one after show.
      var heads = panel('.ca-thread-toollabel').map(function (h) { return h.textContent.split(' · ')[0]; });
      check('the calls before it fold and it stays in view', panel('.ca-thread-runhead').length === 1 && /^2 tool calls/.test(panel('.ca-thread-runhead')[0].textContent.trim()) && heads.join(',') === 'Bash,Artifact,Bash,SendUserFile,View file', heads.join(','));
      made[0].querySelector('a').click();
      return until(function () { return opened().length === 1; }, 3000).then(function () {
        check('a click on it opens the page', opened()[0][1].url === URL, JSON.stringify(opened()[0][1]));
        check('and leaves its call closed', !tool.querySelector('.ca-thread-tooldetail'));
        tool.querySelector('.ca-thread-toolhead').click();
        return until(function () { return tool.querySelector('.ca-thread-out'); }, 3000);
      }).then(function () {
        var pre = tool.querySelector('.ca-thread-out');
        var links = Array.prototype.slice.call(pre.querySelectorAll('a'));
        check('its output keeps its text', pre.textContent === PUBLISHED);
        check('with its link a link', links.length === 1 && links[0].textContent === URL, links.length + '');
        links[0].click();
        return until(function () { return opened().length === 2; }, 3000).then(function () {
          check('which opens too', opened()[1][1].url === URL);
        });
      });
    })
    .then(function () {
      // Files a call sent you: listed under its line with what it said of them, each opening.
      var sent = panel('.ca-thread-sent');
      check('a call that sent files lists them under its line', sent.length === 1 && /^SendUserFile · dashboard\.png and 1 more/.test(sent[0].closest('.ca-thread-tool').querySelector('.ca-thread-toollabel').textContent), sent.length + '');
      check('with what it said of them', sent[0].querySelector('.ca-thread-sentcaption').textContent === CAPTION);
      var files = Array.prototype.slice.call(sent[0].querySelectorAll('button.ca-md-file'));
      var names = files.map(function (f) { return f.querySelector('.ca-md-file-name').textContent + ' in ' + f.querySelector('.ca-md-file-dir').textContent; });
      check('each by its name and folder', names.join('|') === 'dashboard.png in ~/projects/halyard/tmp|before.png in /private/tmp/claude-501/-Users-me-projects-halyard/20898bb3/scratchpad/gone', names.join('|'));
      var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_file'; }); };
      files[0].click();
      return until(function () { return opens().length === 1; }, 3000).then(function () {
        check('a click opens the file', opens()[0][1].path === SENT[0], JSON.stringify(opens()[0][1]));
        check('and leaves its call closed', !sent[0].closest('.ca-thread-tool').querySelector('.ca-thread-tooldetail'));
        files[1].click();
        return until(function () { return sent[0].querySelector('.ca-md-file-why'); }, 3000);
      }).then(function () {
        check('one that’s gone says so in place', sent[0].querySelector('.ca-md-file-why').textContent === 'before.png isn’t on this machine any more.', sent[0].querySelector('.ca-md-file-why').textContent);
      });
    })
    .then(function () {
      // The folded calls, opened: a link in JSON ends where its string does.
      panel('.ca-thread-runhead')[0].click();
      return until(function () { return panel('.ca-thread-toollabel').length === 7; }, 3000).then(function () {
        var read = panel('.ca-thread-toolhead').filter(function (h) { return /^Read/.test(h.textContent.trim()); })[0];
        read.click();
        return until(function () { return read.parentElement.querySelector('.ca-thread-out'); }, 3000).then(function () {
          var links = Array.prototype.slice.call(read.parentElement.querySelectorAll('.ca-thread-out a')).map(function (a) { return a.textContent; });
          check('links in JSON end at their quotes', links.join(' ') === 'https://example.com/a_b https://example.com/c', links.join(' '));
          check('a call’s input has no links', read.parentElement.querySelectorAll('.ca-thread-in a').length === 0);
          // The file it read, by its name in its line: a click on the name opens it and leaves
          // the call as it was; a right-click on the line offers the file's menu.
          var name = read.querySelector('.ca-thread-toolfile');
          check('a call that read a file names it as a link', !!name && name.textContent === 'slots.json' && name.title === 'Open ' + READ, name ? name.outerHTML : 'none');
          var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_file' && c[1].path === READ; }); };
          name.click();
          return until(function () { return opens().length === 1; }, 3000).then(function () {
            check('a click on the name opens the file', true);
            check('and leaves its call open as it was', !!read.parentElement.querySelector('.ca-thread-out'));
            // Each is a button of its own, neither inside another, for the keyboard and a screen
            // reader: the name, and the chevron, which says whether the call is open.
            var toggle = read.querySelector('.ca-thread-tooltoggle');
            check('the name is a button of its own', name.tagName === 'BUTTON' && !name.parentElement.closest('button'), name.parentElement.outerHTML.slice(0, 120));
            check('and so is the chevron, saying the call is open', !!toggle && toggle.tagName === 'BUTTON' && !toggle.parentElement.closest('button') && toggle.getAttribute('aria-expanded') === 'true', toggle ? toggle.outerHTML.slice(0, 120) : 'none');
            toggle.click();
            return until(function () { return !read.parentElement.querySelector('.ca-thread-out'); }, 3000).then(function () {
              check('which closes it', toggle.getAttribute('aria-expanded') === 'false');
              toggle.click();
              return until(function () { return read.parentElement.querySelector('.ca-thread-out'); }, 3000);
            });
          }).then(function () {
            // In a narrow panel the name's end is what's cut, with an ellipsis, not the whole name.
            var line = read.parentElement;
            line.style.width = '130px';
            var shown = name.querySelector('code').getBoundingClientRect().width;
            var cut = name.querySelector('code').scrollWidth > name.querySelector('code').clientWidth;
            check('a narrow line cuts the name’s end, keeping its start', shown > 12 && cut, 'shown ' + shown + 'px, cut ' + cut);
            line.style.width = '';
            // A long summary before the name gives up its room first.
            var view = panel('.ca-thread-toolhead').filter(function (h) { return /^View file/.test(h.textContent.trim()); })[0];
            var viewName = view && view.querySelector('.ca-thread-toolfile');
            check('a line that names its file after what it was for names it', !!viewName && viewName.textContent === 'config.toml', view ? view.textContent : 'none');
            view.parentElement.style.width = '320px';
            var room = viewName.getBoundingClientRect().width;
            check('and keeps the name in view on a narrow line, cutting the words before it', room > 40 && view.querySelector('.ca-thread-toolpart').scrollWidth > view.querySelector('.ca-thread-toolpart').clientWidth, 'name ' + room + 'px');
            view.parentElement.style.width = '';
            return menuOf(read);
          }).then(function (labels) {
            check('a right-click on its line offers the file’s menu', labels.indexOf('Open') >= 0 && labels.indexOf('Reveal in Finder') >= 0 && labels.indexOf('Copy path') >= 0, labels.join(' | '));
            // Closed again, so a screenshot shows the fold.
            panel('.ca-thread-runhead')[0].click();
          });
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
