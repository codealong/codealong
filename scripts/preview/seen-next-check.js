// Checks marking seen and going on in the Sessions view's panel (App.tsx `seenAndOn`), and the
// panel expanded to the whole window (⌃F): two sessions that need you, the first on a reply that
// asks you something and the second on a call to allow, and two whose turns ended. The list's
// letter keys take ⌃ here, so a word typed on the list does nothing. ⌃E on the list marks seen and
// goes on with the keys left on the list, a session with nothing to mark stays, ⌃E and ⌘E in the
// box do it from there, ⌃J in the box moves along with the cursor in the next box, and the check
// on the panel does it too. ⌃F expands the panel, Escape in its box leaves the keys with the
// window, ⌃J still moves along, and ⌃F puts the list back.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/seen-next-check.js
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
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3, asks: true },
    { status: 'needs', since: now - 3600e3, asks: false },
    { status: 'unread', since: now - 5 * 60e3, asks: false },
    { status: 'unread', since: now - 10 * 60e3, asks: false },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked[1] && (args || {}).key === picked[1].key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'p1', label: 'Run `git push`', input: 'git push origin HEAD:main', cannot: null });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = p.asks;
          s.hosted = false;
          // No turn of theirs runs, or Escape in the box would stop it rather than give back the keys.
          s.stoppable = false;
          s.title = 'Queue ' + 'ABCD'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var list = function () { return document.querySelector('[role="listbox"]'); };
  var body = function () { return document.querySelector('.ca-win-body'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var seenKeys = function () { return window.__calls.filter(function (c) { return c[0] === 'mark_seen'; }).map(function (c) { return c[1].keys.join(','); }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('seen next check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return wait(50); });
    })
    .then(function () {
      check('clicking a row puts the cursor in its box', document.activeElement === box());
      key(box(), { key: 'Escape' });
      check('the list has the keys', document.activeElement === list());
      var calls = window.__calls.length;
      var stored = JSON.stringify(localStorage);
      'a pear fever'.split('').forEach(function (c) { key(list(), { key: c }); });
      return wait(300).then(function () {
        check('plain letters on the list do nothing', title() === 'Queue A' && window.__calls.length === calls && JSON.stringify(localStorage) === stored && !document.querySelector('.ca-renamedlg'), title() + ' ' + window.__calls.slice(calls).map(function (c) { return c[0]; }).join(','));
        key(list(), { key: 'e', ctrlKey: true });
        return on('Queue B');
      });
    })
    .then(function () {
      check('⌃E marks a reply that asks you seen and goes on', title() === 'Queue B' && seenKeys().join(';') === picked[0].key, title() + ' ' + seenKeys().join(';'));
      check('and says so', /Queue A: marked seen/.test(strip()), strip());
      check('the keys stay with the list', document.activeElement === list(), document.activeElement && document.activeElement.className);
      check('a call to allow has no check', !document.querySelector('.ca-sessinsp-seen'));
      key(list(), { key: 'e', ctrlKey: true });
      return wait(300);
    })
    .then(function () {
      check('⌃E on a call to allow does nothing', title() === 'Queue B' && seenKeys().length === 1, title());
      key(list(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue C').then(function () { return wait(50); });
    })
    .then(function () {
      check('⌘] goes on to a finished turn with the cursor in its box', title() === 'Queue C' && document.activeElement === box(), title());
      key(box(), { key: 'e', ctrlKey: true });
      return on('Queue D').then(function () { return wait(50); });
    })
    .then(function () {
      check('⌃E in the box marks seen and goes on', title() === 'Queue D' && seenKeys()[1] === picked[2].key, title() + ' ' + seenKeys().join(';'));
      check('the next one’s box has the cursor', document.activeElement === box());
      var seenBtn = document.querySelector('.ca-sessinsp-seen');
      check('the check says it goes on', !!seenBtn && /go on to the next \(⌃E\)/.test(seenBtn.title), seenBtn && seenBtn.title);
      seenBtn.click();
      return on('Queue B');
    })
    .then(function () {
      check('the check marks seen and goes on to what’s left', title() === 'Queue B' && seenKeys()[2] === picked[3].key, title() + ' ' + seenKeys().join(';'));
      check('not expanded yet', !body().classList.contains('is-panel-max'));
      box().focus();
      var ev = new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true, cancelable: true });
      box().dispatchEvent(ev);
      check('⌃F in the box isn’t the box’s', ev.defaultPrevented);
      return wait(200);
    })
    .then(function () {
      var panel = document.querySelector('.ca-sessinsp').getBoundingClientRect();
      var board = document.querySelector('.ca-board');
      var btn = document.querySelector('.ca-sessinsp-max');
      check('⌃F expands the panel', body().classList.contains('is-panel-max'));
      check('the list and sidebar step aside', getComputedStyle(board).display === 'none' && getComputedStyle(document.querySelector('.ca-side, .ca-rail')).display === 'none');
      check('the panel takes the window', panel.width >= window.innerWidth - 2, panel.width + ' of ' + window.innerWidth);
      check('its button says how to go back', !!btn && btn.getAttribute('aria-pressed') === 'true' && /Show the list again \(⌃F\)/.test(btn.title), btn && btn.title);
      check('and it’s remembered', localStorage.getItem('codealong.sessionPanelMax') === '1');
      var head = document.querySelector('.ca-sessinsp-head');
      var pad = parseFloat(getComputedStyle(head).paddingLeft);
      check('its parts keep to a readable width', pad > 100, pad + 'px');
      box().focus();
      key(box(), { key: 'Escape' });
      check('Escape in the box leaves the keys with the window', document.activeElement === document.body, document.activeElement && document.activeElement.className);
      key(document.body, { key: 'j', ctrlKey: true });
      return wait(300);
    })
    .then(function () {
      check('⌃J still moves along the list while expanded', title() !== 'Queue B' && body().classList.contains('is-panel-max'), title());
      var was = title();
      box().focus();
      key(box(), { key: 'k', ctrlKey: true });
      return wait(300).then(function () {
        check('⌃K in the box moves along, the cursor in the next one’s box', title() !== was && document.activeElement === box(), title());
        key(box(), { key: 'f', ctrlKey: true });
        return wait(200);
      });
    })
    .then(function () {
      check('⌃F puts the list back', !body().classList.contains('is-panel-max') && getComputedStyle(document.querySelector('.ca-board')).display !== 'none');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
