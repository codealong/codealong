// Checks the links in a session's conversation that aren't in a reply: a call that made a page on
// the web (an artifact Claude Code published) says so under its line, as a link that opens it,
// and stays in view while the calls around it fold; and web links in a tool's output open too,
// with the output's text as it was. The conversation is a stand-in, on the first session:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/thread-links-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=thread-links) it leaves the panel open on it, the publish's
// output showing, for a screenshot.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };

  localStorage.setItem('codealong.project', 'All projects');
  var URL = 'https://claude.ai/artifact/VuawMfgy2qCBAFhpc6dC3H';
  var FILE = '/private/tmp/claude-501/-Users-me-projects-osprey/20898bb3/scratchpad/security-dashboard.html';
  var PUBLISHED = 'Published ' + FILE + ' at ' + URL + ' (Version 9, version id 1790960406-8f4f)\n\nLive subscription: already connected from earlier in this session.\n\nTo update: publish the same file path here (keeps this URL).';
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
        item('tool', 'Read · `slots.json`', 4, { id: 't2', input: '/x/slots.json' }),
        item('result', '', 5, { id: 't2', output: '{"docs":"https://example.com/a_b","next":"https://example.com/c"}' }),
        item('tool', 'Bash · `tail -12 build.log`', 6, { id: 't3', input: 'tail -12 build.log' }),
        item('result', '', 7, { id: 't3', output: 'done' }),
        item('tool', 'Artifact · `security-dashboard.html`', 8, { id: 't4', input: JSON.stringify({ file_path: FILE, label: 'Osprey current 2 Oct' }, null, 2) }),
        item('result', '', 9, { id: 't4', output: PUBLISHED, link: { url: URL, title: 'Osprey security dashboard', note: 'Updated · version 9' } }),
        item('tool', 'Bash · `git status --short`', 10, { id: 't5', input: 'git status --short' }),
        item('result', '', 11, { id: 't5', output: '' }),
        item('agent', 'Everything checks out, and the dashboard is published.', 12),
      ] });
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
      check('the publish says what it made', made.length === 1 && made[0].querySelector('a').textContent.trim() === 'Osprey security dashboard', made.map(function (m) { return m.textContent; }).join('|'));
      check('and what it did with it', /Updated · version 9/.test(made[0].querySelector('.ca-meta').textContent));
      var tool = made[0].closest('.ca-thread-tool');
      check('under its own call’s line', /^Artifact · security-dashboard\.html/.test(tool.querySelector('.ca-thread-toollabel').textContent));
      // Three calls ahead of it fold into a line and the latest of them; it and the one after show.
      var heads = panel('.ca-thread-toollabel').map(function (h) { return h.textContent.split(' · ')[0]; });
      check('the calls before it fold and it stays in view', panel('.ca-thread-runhead').length === 1 && /^2 tool calls/.test(panel('.ca-thread-runhead')[0].textContent.trim()) && heads.join(',') === 'Bash,Artifact,Bash', heads.join(','));
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
      // The folded calls, opened: a link in JSON ends where its string does.
      panel('.ca-thread-runhead')[0].click();
      return until(function () { return panel('.ca-thread-toollabel').length === 5; }, 3000).then(function () {
        var read = panel('.ca-thread-toolhead').filter(function (h) { return /^Read/.test(h.textContent.trim()); })[0];
        read.click();
        return until(function () { return read.parentElement.querySelector('.ca-thread-out'); }, 3000).then(function () {
          var links = Array.prototype.slice.call(read.parentElement.querySelectorAll('.ca-thread-out a')).map(function (a) { return a.textContent; });
          check('links in JSON end at their quotes', links.join(' ') === 'https://example.com/a_b https://example.com/c', links.join(' '));
          check('a call’s input has no links', read.parentElement.querySelectorAll('.ca-thread-in a').length === 0);
          // Closed again, so a screenshot shows the fold.
          panel('.ca-thread-runhead')[0].click();
        });
      });
    })
    .then(function () {
      say('done: ' + out.join('; '));
    })
    .catch(function (e) {
      say('done: ' + out.join('; ') + ' ERROR ' + e.message);
    });
})();
