// Checks the panel's Always for a call held through the plugin's hook (asks.rs): a session
// waiting on a call Claude Code offers to allow from now on shows Always beside Deny and Allow,
// with what it adds on hover, and clicking it answers `always`.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/always-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=always) it leaves the panel open for a screenshot.
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
  var picked = null;
  var rule = 'Allows working in `~/projects/app` for this session; Switches to accepting edits for this session.';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'ask-1', label: 'Bash · `touch alpha.txt`', input: 'touch alpha.txt', cannot: null, always: true, alwaysRule: rule });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Allowed Bash · `touch alpha.txt`, and from now on.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = 'Bash · `touch alpha.txt`';
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var button = function (text) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-composer.is-approval button'), function (b) { return b.textContent.trim() === text; })[0];
  };

  say('always check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Always'); }, 5000);
    })
    .then(function () {
      var always = button('Always');
      check('always shown', !!always && !!button('Deny') && !!button('Allow'));
      check('says what it adds', always.getAttribute('title') === rule, always.getAttribute('title'));
      if (window.__always_screenshot) return;
      always.click();
      return until(function () { return window.__calls.some(function (c) { return c[0] === 'session_answer'; }); }, 3000).then(function () {
        var c = window.__calls.filter(function (c) { return c[0] === 'session_answer'; })[0][1];
        check('answers always', c.answer === 'always' && c.id === 'ask-1' && c.key === picked.key, JSON.stringify(c));
      });
    })
    .then(function () { say('done: ' + out.join(', ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(', ')); });
})();
