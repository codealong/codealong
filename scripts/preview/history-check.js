// Checks going back and forward through the sessions you looked at (history.ts, App.tsx
// `travel`): ⌃- and ⌃⇧- from a session's box, the search field and another view, the title
// bar's buttons and what they say, a session passed over quickly left out, one only just come to
// kept for Forward, and the search left alone when the list has the session as it's filtered.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/history-check.js
(function () {
  // This runs before the app's script: four of the board's real sessions get names to go by, and
  // the window starts with no history.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionHistory');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= 4) return;
          seen[s.key] = true;
          s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.lastActivity = Date.now() - 60e3 * (picked.length + 1);
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Place ' + 'ABCD'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  // Longer than history.ts's DWELL_MS: a session looked at.
  var LOOK = 1300;
  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var search = function () { return document.querySelector('.ca-listfind input'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var click = function (i) { row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true })); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var back = function () { key({ key: '-', code: 'Minus', ctrlKey: true }); };
  var forward = function () { key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true }); };
  var button = function (name) { return document.querySelector('.ca-histnav button[aria-label="' + name + '"]'); };
  var off = function (name) { return button(name).getAttribute('aria-disabled') === 'true'; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var look = function (i, name) {
    click(i);
    return on(name).then(function () { return wait(LOOK); });
  };

  say('history check: waiting for the list');
  until(function () { return picked.length === 4 && row(0) && button('Back'); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      check('with nowhere to go, both buttons are off', off('Back') && off('Forward'));
      check('and say their keys', /\(⌃-\)$/.test(button('Back').title) && /\(⌃⇧-\)$/.test(button('Forward').title), button('Back').title + ' / ' + button('Forward').title);
      return look(0, 'Place A');
    })
    .then(function () { return look(1, 'Place B'); })
    .then(function () { return look(2, 'Place C'); })
    .then(function () {
      check('Back says where it goes', button('Back').title === 'Back to Place B (⌃-)' && !off('Back'), button('Back').title);
      check('Forward is off on the newest', off('Forward'));
      check('the cursor is in the box', document.activeElement === box());
      back();
      return on('Place B').then(function () { return wait(50); });
    })
    .then(function () {
      check('⌃- from the box goes back a session', title() === 'Place B', title());
      check('and the cursor goes to its box', document.activeElement === box());
      back();
      return on('Place A');
    })
    .then(function () {
      check('⌃- again goes back another', title() === 'Place A', title());
      check('Forward says where it goes', button('Forward').title === 'Forward to Place B (⌃⇧-)', button('Forward').title);
      forward();
      return on('Place B');
    })
    .then(function () {
      check('⌃⇧- goes forward', title() === 'Place B', title());
      button('Back').click();
      return on('Place A');
    })
    .then(function () {
      check('the Back button goes back', title() === 'Place A', title());
      button('Forward').click();
      return on('Place B');
    })
    .then(function () {
      check('the Forward button goes forward', title() === 'Place B', title());
      return wait(LOOK);
    })
    .then(function () {
      check('going back and forward adds nothing to the list', button('Forward').title === 'Forward to Place C (⌃⇧-)' && button('Back').title === 'Back to Place A (⌃-)', button('Back').title + ' / ' + button('Forward').title);
      // D, only just come to: Back keeps it for Forward, and C, which was ahead, is dropped.
      click(3);
      return on('Place D');
    })
    .then(function () {
      back();
      return on('Place B');
    })
    .then(function () {
      check('⌃- from a session only just come to goes back', title() === 'Place B', title());
      check('and keeps it for Forward, in place of what was ahead', button('Forward').title === 'Forward to Place D (⌃⇧-)', button('Forward').title);
      forward();
      return on('Place D').then(function () { return wait(LOOK); });
    })
    .then(function () {
      // A passed over on the way to C.
      click(0);
      return on('Place A');
    })
    .then(function () { return look(2, 'Place C'); })
    .then(function () {
      check('a session passed over quickly isn’t a stop on the way back', button('Back').title === 'Back to Place D (⌃-)', button('Back').title);
      // From a view that shows no session, Back is to the last one.
      key({ key: 'Tab', code: 'Tab', ctrlKey: true });
      return until(function () { return !title(); }, 3000).catch(function () {});
    })
    .then(function () {
      check('another view shows no session', !title(), title());
      check('where Back is to the last one', button('Back').title === 'Back to Place C (⌃-)', button('Back').title);
      back();
      return on('Place C');
    })
    .then(function () {
      check('⌃- from another view shows the last session, in Sessions', title() === 'Place C' && !!row(2), title());
      // A search that lists the session stays; one that hides it is cleared. The list's find
      // (ListBar.tsx) opens as ⌥⌘F opens it.
      key({ key: 'ƒ', code: 'KeyF', metaKey: true, altKey: true });
      return until(function () { return !!search() && document.activeElement === search(); }, 2000, 'the list’s find');
    })
    .then(function () {
      type(search(), 'Place');
      return wait(100);
    })
    .then(function () {
      back();
      return on('Place D');
    })
    .then(function () {
      check('⌃- works from the search field, and a search that lists the session stays', title() === 'Place D' && search().value === 'Place', title() + ' / ' + search().value);
      type(search(), 'Place D');
      return wait(100);
    })
    .then(function () {
      back();
      return on('Place B');
    })
    .then(function () {
      check('a search that hides the session is cleared', title() === 'Place B' && (!search() || search().value === ''), title() + ' / ' + (search() && search().value));
      var kept = JSON.parse(localStorage.getItem('codealong.sessionHistory') || 'null');
      check('the history is kept for the next launch', !!kept && kept.keys[kept.at] === picked[1].key, JSON.stringify(kept && kept.at));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
