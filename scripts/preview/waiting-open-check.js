// Checks the quick panel as the shortcut that goes through what waits on you opens it, when the
// bar isn't docked in the notch (quick.rs `work_through`, Quick.tsx `answerFirst`): three
// sessions wait on you, and the panel opens on the longest wait, answering it with the rest to
// go through; ⌘] goes on to the next. With nothing waiting it opens on its list.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/waiting-open-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };

  localStorage.setItem('codealong.quick.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'unread', since: now - 10 * 60e3 },
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 3600e3 },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          // The same ones on every read of the board.
          var n = picked.indexOf(s.key);
          if (n < 0) {
            if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
            n = picked.length;
            picked.push(s.key);
          }
          seen[s.key] = true;
          var p = plan[n];
          s.status = p.status;
          s.statusSince = p.since;
          s.asks = p.status === 'needs';
          s.title = 'Queue ' + 'CAB'[n];
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var board;

  say('waiting open check: waiting for the panel');
  until(function () { return picked.length === plan.length && window.__quickOpen && document.querySelectorAll('.ca-quick-row, [role="option"]').length > 0; }, 20000)
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (b) {
      board = b;
      check('the list is up first', !title());
      window.__quickOpen({ waiting: true }, board);
      return on('Queue A');
    })
    .then(function () {
      check('the shortcut opens on the longest wait', title() === 'Queue A', title());
      check('with the rest to go through', /2 more waiting on you/.test(strip()), strip());
      window.dispatchEvent(new KeyboardEvent('keydown', { key: ']', code: 'BracketRight', metaKey: true, bubbles: true, cancelable: true }));
      return on('Queue B');
    })
    .then(function () {
      check('⌘] goes on to the next', title() === 'Queue B', title());
      // Nothing waits: the panel opens on its list.
      var calm = JSON.parse(JSON.stringify(board));
      calm.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.status === 'needs' || s.status === 'unread') s.status = 'idle'; }); });
      window.__quickOpen({ waiting: true }, calm);
      return wait(300);
    })
    .then(function () {
      check('with nothing waiting it opens on its list', !title() && !!document.querySelector('.ca-quick-input textarea'), title());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
