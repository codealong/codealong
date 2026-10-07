// Checks Show in Codealong on a session, asked from another window (the floating bar, the quick
// panel, a card on the desktop: App.tsx `reveal`): the main window goes to Sessions with that
// session selected and its conversation in the panel. The session is made five days old, with
// the age filter at 24 hours, so the filter has to widen as little as lists it
// (boardLists.ts `ageListingSession`), or Sessions selects another session in its place; one
// that's active today shows with no change to the filter.
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/session-reveal-check.js
(function () {
  var DAY = 86400e3;
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', '1d');
  var plan = [{ title: 'Revealed today', ago: 60e3 }, { title: 'Revealed after days', ago: 5 * DAY }];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var now = Date.now();
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          var at = picked.filter(function (x) { return x.key === s.key; })[0];
          if (!at && (picked.length >= plan.length || s.visitor || s.archived || l.hidden || l.device)) return;
          if (!at) {
            at = { key: s.key, lane: l.id, title: plan[picked.length].title, ago: plan[picked.length].ago };
            picked.push(at);
          }
          s.title = at.title;
          s.lastActivity = now - at.ago;
          s.lastPromptAt = Math.min(s.lastPromptAt || s.lastActivity, s.lastActivity);
        });
      });
      // Not pinned, or Sessions would list them whatever their age.
      var keys = picked.map(function (x) { return x.key; });
      board.pinned = (board.pinned || []).filter(function (k) { return keys.indexOf(k) < 0; });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var list = function () { var l = document.querySelector('[role="listbox"]'); return l ? l.getAttribute('aria-label') : null; };
  var row = function (x) { return document.getElementById('row-' + x.lane + '::' + x.key); };
  var age = function () { return localStorage.getItem('codealong.age'); };
  var reveal = function (x) {
    window.__emit('reveal', { kind: 'session', laneId: x.lane, key: x.key });
    return until(function () { return title() === x.title; }, 3000).catch(function () {});
  };

  say('session reveal check: waiting for the board');
  until(function () { return picked.length === plan.length && !!list(); }, 20000)
    .catch(function () { throw new Error('not two sessions on the board to show'); })
    .then(function () { return wait(300); })
    .then(function () { return reveal(picked[0]); })
    .then(function () {
      check('a session active today shows in Sessions', list() === 'Sessions' && title() === picked[0].title, list() + ' / ' + title());
      check('with no change to the age filter', age() === '1d', age());
      return reveal(picked[1]);
    })
    .then(function () {
      check('one older than the age filter shows, not another in its place', title() === picked[1].title, title());
      check('widening the filter as little as lists it', age() === '7d', age());
      var r = row(picked[1]);
      check('its row is listed and selected', !!r && r.getAttribute('aria-selected') === 'true', r ? r.outerHTML.slice(0, 160) : 'no row');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
