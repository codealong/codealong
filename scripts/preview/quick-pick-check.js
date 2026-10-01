// Checks what picking a session in the quick panel does (Quick.tsx): ⏎, a click and ⌘1 show it
// in Codealong and put the panel away with its search cleared, without opening it where it
// lives; ⌥⏎ opens it where it lives, without showing it in Codealong.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-pick-check.js
// Every step should say ok. The board needs a session in a worktree that isn't hidden.
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
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL' + (why ? ' (' + why + ')' : ''))); };

  // This runs before the app's script: one of the board's real sessions gets a title nothing
  // else has, so a search finds it alone, and its worktree says it can open.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var WORD = 'zebrafinch';
  var TITLE = 'Pick the ' + WORD;
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked ? picked.key !== s.key || picked.laneId !== l.id : l.hidden || s.visitor || s.archived) return;
          s.title = TITLE;
          l.targets.forEach(function (t) { if (t.id === s.openTarget) t.disabled = false; });
          picked = { laneId: l.id, key: s.key, target: s.openTarget };
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1]; }); };
  var last = function (cmd) { return calls(cmd).pop(); };
  var row = function () {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-quick-row'), function (r) { return r.textContent.indexOf(TITLE) >= 0; })[0];
  };
  // The search finds the session and it's the row ⏎ does.
  var find = function () {
    type(input(), WORD);
    return until(function () { var r = row(); return !!r && r.classList.contains('is-active'); }, 5000).then(function () { return wait(100); });
  };
  var isPicked = function (shown) { return !!shown && !!shown.reveal && shown.reveal.kind === 'session' && shown.reveal.laneId === picked.laneId && shown.reveal.key === picked.key; };
  var counts = function () { return { shows: calls('show_main').length, opens: calls('open_from_bar').length, hides: calls('hide_quick').length }; };
  var before;
  // One pick: what it should have shown, or opened, and nothing of the other.
  var shows = function (name) {
    var now = counts();
    check(name + ' shows the session in Codealong', now.shows === before.shows + 1 && isPicked(last('show_main')), JSON.stringify(last('show_main')));
    check(name + ' doesn’t open it where it lives', now.opens === before.opens);
    check(name + ' puts the panel away', now.hides === before.hides + 1);
  };

  say('quick pick check: waiting for the list');
  until(function () { return !!picked && !!input() && document.querySelectorAll('.ca-quick-row').length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(find)
    .then(function () {
      check('the row says what the keys do', /⏎ shows it in Codealong/.test(row().title) && /⌥⏎ opens it where it lives/.test(row().title), row().title);
      before = counts();
      key(input(), { key: 'Enter' });
      return wait(300);
    })
    .then(function () {
      shows('⏎');
      check('and clears the search', input().value === '', input().value);
      return find();
    })
    .then(function () {
      before = counts();
      key(input(), { key: 'Enter', altKey: true });
      return wait(300);
    })
    .then(function () {
      var now = counts();
      var o = last('open_from_bar');
      check('⌥⏎ opens it where it lives', now.opens === before.opens + 1 && o.laneId === picked.laneId && o.targetId === picked.target, JSON.stringify(o));
      check('⌥⏎ doesn’t show it in Codealong', now.shows === before.shows);
      check('⌥⏎ puts the panel away', now.hides === before.hides + 1);
      return find();
    })
    .then(function () {
      before = counts();
      row().click();
      return wait(300);
    })
    .then(function () {
      shows('a click');
      return find();
    })
    .then(function () {
      before = counts();
      key(window, { key: '1', code: 'Digit1', metaKey: true });
      return wait(300);
    })
    .then(function () { shows('⌘1'); })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
