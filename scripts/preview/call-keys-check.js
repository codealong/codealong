// Checks the keys for a call a session waits on, in the Sessions view's panel (SessionReply.tsx
// `Approval`): one session waits on a call and nothing else waits. Allow and Deny say their keys;
// ⌘⌫ in the box with words in it is the box's, and in an empty box denies and stays; ⌘⏎ allows,
// and with nothing after it to go on to, stays.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/call-keys-check.js
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

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var boardNow = null;
  var asked = 0;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      asked += 1;
      return Promise.resolve({ id: 'p' + asked, label: 'Run `git push`', input: 'git push origin HEAD:main', cannot: null });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(args.answer === 'deny' ? 'Denied.' : 'Allowed.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.lastActivity = s.statusSince;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Queue A';
          picked = { key: s.key, lane: l.id };
        });
      });
      boardNow = board;
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var note = function () { var t = document.querySelector('.ca-sessinsp .ca-composer:not(.is-approval) .ca-composer-note'); return t ? t.textContent : ''; };
  var button = function (text) { return Array.prototype.filter.call(approval().querySelectorAll('button'), function (b) { return b.textContent.indexOf(text) === 0; })[0]; };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }).map(function (c) { return c[1]; }); };

  say('call keys check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Queue A' && approval() && button('Allow'); }, 5000);
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('Allow says ⌘⏎', /⌘⏎/.test(button('Allow').textContent), button('Allow').textContent);
      check('Deny says ⌘⌫', /⌘⌫/.test(button('Deny').textContent), button('Deny').textContent);
      check('the box says ⌘⏎ allows, with nothing to go on to', /^⌘⏎ allow · /.test(note()), note());
      check('the cursor is in the box', document.activeElement === box());
      type(box(), 'Use the other remote');
      key(box(), { key: 'Backspace', metaKey: true });
      return wait(300);
    })
    .then(function () {
      check('⌘⌫ with words in the box is the box’s', answers().length === 0, JSON.stringify(answers()));
      type(box(), '');
      key(box(), { key: 'Backspace', metaKey: true });
      return until(function () { return answers().length === 1; }, 3000).then(function () { return wait(200); });
    })
    .then(function () {
      var a = answers()[0];
      check('⌘⌫ in an empty box denies', a.key === picked.key && a.answer === 'deny', JSON.stringify(a));
      check('and stays', title() === 'Queue A', title());
      // It asks again: a new call.
      var b = JSON.parse(JSON.stringify(boardNow));
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) s.statusSince += 1000; }); });
      window.__emit('board', b);
      return until(function () { return asked >= 2 && approval() && button('Allow'); }, 5000).then(function () { return wait(100); });
    })
    .then(function () {
      box().focus();
      key(box(), { key: 'Enter', metaKey: true });
      return until(function () { return answers().length === 2; }, 3000).then(function () { return wait(200); });
    })
    .then(function () {
      var a = answers()[1];
      check('⌘⏎ allows, with nothing else waiting', a.key === picked.key && a.answer === 'allow', JSON.stringify(a));
      check('and stays', title() === 'Queue A', title());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
