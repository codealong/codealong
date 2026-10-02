// Checks the Sessions view's keys and working through what waits on you from its panel (App.tsx
// `seenAndOn`, `goOn`, `goBack`), with the panel expanded to the whole window (⇧⌘F): two sessions
// that need you, the first on a reply that asks you something and the second on a call to allow,
// and two whose turns ended. A click puts the cursor in the box, and the list's keys are chords
// that work from there too, so a word typed on the list does nothing. ⌥⇧↓ ⌥⇧↑ go through what
// waits on you, ⌘E marks seen and goes on (the keys staying where they were), ⌘] skips, ⌃⌘P ⌃⌘R
// ⌃⌘A pin, rename and archive, ⇧⌘F expands, ⌥↑ ⌥↓ move along, ⌃Tab goes through the views, and
// the right-click menu shows the chords.
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
      // None pinned, whatever is today: ⌃⌘P pins the one it's pressed on.
      board.pinned = [];
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

  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var boxed = function () { return document.activeElement === box(); };
  var press = function (el, init) {
    var ev = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init));
    el.dispatchEvent(ev);
    return ev.defaultPrevented;
  };
  var named = function (name) { return on(name).then(function () { return wait(50); }); };

  say('seen next check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return named('Queue A');
    })
    .then(function () {
      check('clicking a row puts the cursor in its box', boxed());
      key(box(), { key: 'ArrowDown', altKey: true, shiftKey: true });
      return named('Queue B');
    })
    .then(function () {
      check('⌥⇧↓ in the box goes on to the next that waits on you', title() === 'Queue B' && boxed(), title());
      key(box(), { key: 'ArrowUp', altKey: true, shiftKey: true });
      return named('Queue A');
    })
    .then(function () {
      check('⌥⇧↑ goes back to the one before', title() === 'Queue A' && boxed(), title());
      key(box(), { key: 'ArrowUp', altKey: true, shiftKey: true });
      return named('Queue D');
    })
    .then(function () {
      check('and from the first, round to the last', title() === 'Queue D', title());
      key(box(), { key: 'ArrowDown', altKey: true, shiftKey: true });
      return named('Queue A');
    })
    .then(function () {
      key(box(), { key: 'Escape' });
      check('Escape gives the list the keys', document.activeElement === list());
      var n = window.__calls.length;
      var stored = JSON.stringify(localStorage);
      'a pear fever'.split('').forEach(function (c) { key(list(), { key: c }); });
      return wait(300).then(function () {
        check('plain letters on the list do nothing', title() === 'Queue A' && window.__calls.length === n && JSON.stringify(localStorage) === stored && !document.querySelector('.ca-renamedlg'), title() + ' ' + window.__calls.slice(n).map(function (c) { return c[0]; }).join(','));
        key(list(), { key: 'e', metaKey: true });
        return on('Queue B');
      });
    })
    .then(function () {
      check('⌘E on the list marks a reply that asks you seen and goes on', title() === 'Queue B' && seenKeys().join(';') === picked[0].key, title() + ' ' + seenKeys().join(';'));
      check('and says so', /Queue A: marked seen/.test(strip()), strip());
      check('the keys stay with the list', document.activeElement === list(), document.activeElement && document.activeElement.className);
      check('a call to allow has no check', !document.querySelector('.ca-sessinsp-seen'));
      key(list(), { key: 'e', metaKey: true });
      return wait(300);
    })
    .then(function () {
      check('⌘E on a call to allow does nothing', title() === 'Queue B' && seenKeys().length === 1, title());
      key(list(), { key: ']', code: 'BracketRight', metaKey: true });
      return named('Queue C');
    })
    .then(function () {
      check('⌘] goes on to a finished turn with the cursor in its box', title() === 'Queue C' && boxed(), title());
      key(box(), { key: 'e', metaKey: true });
      return named('Queue D');
    })
    .then(function () {
      check('⌘E in the box marks seen and goes on', title() === 'Queue D' && seenKeys()[1] === picked[2].key, title() + ' ' + seenKeys().join(';'));
      check('the next one’s box has the cursor', boxed());
      var seenBtn = document.querySelector('.ca-sessinsp-seen');
      check('the check says it goes on', !!seenBtn && /go on to the next \(⌘E\)/.test(seenBtn.title), seenBtn && seenBtn.title);
      seenBtn.click();
      return named('Queue B');
    })
    .then(function () {
      check('the check marks seen and goes on to what’s left', title() === 'Queue B' && seenKeys()[2] === picked[3].key, title() + ' ' + seenKeys().join(';'));
      box().focus();
      var taken = press(box(), { key: 'p', ctrlKey: true, metaKey: true });
      return wait(300).then(function () {
        var pins = calls('set_pinned');
        check('⌃⌘P in the box pins it', taken && pins.length === 1 && pins[0][1].keys.indexOf(picked[1].key) >= 0, JSON.stringify(pins.map(function (c) { return c[1]; })));
        key(box() || document.body, { key: 'p', ctrlKey: true, metaKey: true });
        return wait(300);
      });
    })
    .then(function () {
      var pins = calls('set_pinned');
      check('and again unpins it', pins.length === 2 && pins[1][1].keys.indexOf(picked[1].key) < 0, JSON.stringify(pins.map(function (c) { return c[1]; })));
      return named('Queue B');
    })
    .then(function () {
      box().focus();
      key(box(), { key: 'r', ctrlKey: true, metaKey: true });
      return until(function () { return document.querySelector('.ca-renamedlg'); }, 2000).then(function () { return true; }, function () { return false; });
    })
    .then(function (shown) {
      check('⌃⌘R in the box opens Rename', shown);
      var cancel = Array.prototype.filter.call(document.querySelectorAll('.ca-renamedlg button'), function (b) { return b.textContent.trim() === 'Cancel'; })[0];
      if (cancel) cancel.click();
      return wait(200);
    })
    .then(function () {
      check('not expanded yet', !body().classList.contains('is-panel-max'));
      box().focus();
      check('⇧⌘F in the box isn’t the box’s', press(box(), { key: 'f', shiftKey: true, metaKey: true }));
      return wait(200);
    })
    .then(function () {
      var panel = document.querySelector('.ca-sessinsp').getBoundingClientRect();
      var board = document.querySelector('.ca-board');
      var btn = document.querySelector('.ca-sessinsp-max');
      check('⇧⌘F expands the panel', body().classList.contains('is-panel-max'));
      check('the list and sidebar step aside', getComputedStyle(board).display === 'none' && getComputedStyle(document.querySelector('.ca-side, .ca-rail')).display === 'none');
      check('the panel takes the window', panel.width >= window.innerWidth - 2, panel.width + ' of ' + window.innerWidth);
      check('its button says how to go back', !!btn && btn.getAttribute('aria-pressed') === 'true' && /Show the list again \(⇧⌘F\)/.test(btn.title), btn && btn.title);
      check('and it’s remembered', localStorage.getItem('codealong.sessionPanelMax') === '1');
      var head = document.querySelector('.ca-sessinsp-head');
      var pad = parseFloat(getComputedStyle(head).paddingLeft);
      check('its parts keep to a readable width', pad > 100, pad + 'px');
      box().focus();
      key(box(), { key: 'Escape' });
      check('Escape in the box leaves the keys with the window', document.activeElement === document.body, document.activeElement && document.activeElement.className);
      key(document.body, { key: 'ArrowDown', altKey: true });
      return wait(300);
    })
    .then(function () {
      check('⌥↓ moves along the list while expanded', title() !== 'Queue B' && body().classList.contains('is-panel-max'), title());
      box().focus();
      check('⌥↑ in the box isn’t the box’s', press(box(), { key: 'ArrowUp', altKey: true }));
      return named('Queue B');
    })
    .then(function () {
      check('⌥↑ in the box moves back, the cursor in that one’s box', title() === 'Queue B' && boxed(), title());
      key(box(), { key: 'a', ctrlKey: true, metaKey: true });
      return wait(300);
    })
    .then(function () {
      var archived = calls('archive_session');
      check('⌃⌘A in the box archives it', archived.length === 1 && archived[0][1].key === picked[1].key && archived[0][1].archived === true, JSON.stringify(archived.map(function (c) { return c[1]; })));
      key(document.body, { key: 'f', shiftKey: true, metaKey: true });
      return wait(200);
    })
    .then(function () {
      check('⇧⌘F puts the list back', !body().classList.contains('is-panel-max') && getComputedStyle(document.querySelector('.ca-board')).display !== 'none');
      key(document.body, { key: 'Tab', ctrlKey: true });
      return wait(300);
    })
    .then(function () {
      var label = list() && list().getAttribute('aria-label');
      check('⌃Tab goes to the next view', label !== 'Sessions', label);
      key(document.body, { key: 'Tab', ctrlKey: true, shiftKey: true });
      return wait(300);
    })
    .then(function () {
      var label = list() && list().getAttribute('aria-label');
      check('⌃⇧Tab comes back', label === 'Sessions', label);
      return until(function () { return row(0); }, 3000);
    })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return wait(300);
    })
    .then(function () {
      var made = calls('plugin:menu|new');
      var flat = function (items) { return (items || []).flatMap(function (i) { return i.items ? flat(i.items) : i.accelerator ? [i.text + '=' + i.accelerator] : []; }); };
      var keys = made.length ? flat(made[made.length - 1][1].options.items) : [];
      // The first was marked seen, so its menu has no Mark as seen.
      // Whether it's pinned, and whether it has a terminal to close, are the board's today.
      check('the menu shows the chords', /^Rename…=Ctrl\+Cmd\+R, (Pin|Unpin) session=Ctrl\+Cmd\+P, (For 15 minutes=Ctrl\+Cmd\+1, For 1 hour=Ctrl\+Cmd\+2, For 3 hours=Ctrl\+Cmd\+3, Until [^=]+=Ctrl\+Cmd\+4, )?((Close terminal|End background job)=Ctrl\+Cmd\+W, )?Archive=Ctrl\+Cmd\+A$/.test(keys.join(', ')), keys.join(', '));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
