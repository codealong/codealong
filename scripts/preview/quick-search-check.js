// Checks a search Jev says fits none of the sessions it read (route.ts `suggestions`, Quick.tsx):
// nothing is suggested, the note says it looks like a search, Send to goes under the sessions
// found, and ⏎ shows the session the search found rather than opening Send to.
//   PREVIEW_ROUTE=scripts/preview/route-search-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-search-check.js
// The board needs a session in a worktree that isn't hidden.
(function () {
  // This runs before the app's script: one of the board's real sessions gets a title nothing
  // else has, so the search finds it alone.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var WORDS = 'zebrafinch notes';
  var TITLE = 'Pick the ' + WORDS;
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
          picked = { laneId: l.id, key: s.key };
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row'); };
  var found = function () { return rows().filter(function (r) { return r.textContent.indexOf(TITLE) >= 0; })[0]; };
  var sendTo = function () { return rows().filter(function (r) { return /Send to (a|another) session/.test(r.textContent); })[0]; };
  var noteRow = function () { return q('.ca-quick-row.is-suggest.is-note'); };
  // The first session the search found: the titled one, or one that said the words.
  var firstFound = function () { return rows().filter(function (r) { return !r.classList.contains('is-suggest') && !!r.dataset.row && r.dataset.row !== 'sendto' && r.dataset.row !== 'compose'; })[0]; };
  var expected = null;

  say('quick search check: waiting for the list');
  until(function () { return !!picked && !!input() && rows().length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      type(input(), WORDS);
      return until(function () { return calls('route_quick').length > 0 && !!noteRow() && !noteRow().classList.contains('is-routing') && !!found(); }, 10000);
    })
    .then(function () { return wait(200); })
    .then(function () {
      check('nothing is suggested', all('.ca-quick-row.is-suggest:not(.is-note):not(.is-stale)').length === 0);
      check('the note says it looks like a search', /looks like a search/.test(noteRow().textContent), noteRow().textContent);
      var at = rows().indexOf(sendTo());
      check('Send to comes after the session found', !!sendTo() && at > rows().indexOf(found()), String(at));
      expected = firstFound().dataset.row;
      check('the first session found is the row ⏎ does', firstFound().classList.contains('is-active'), (q('.ca-quick-row.is-active') || {}).textContent);
      key(input(), { key: 'Enter' });
      return wait(300);
    })
    .then(function () {
      var shown = sent('show_main').pop();
      check('⏎ shows the session found', !!shown && !!shown.reveal && shown.reveal.key === expected, expected + ' vs ' + JSON.stringify(shown));
      check('⏎ puts the panel away', calls('hide_quick').length > 0);
    })
    .then(done)
    .catch(stopped);
})();
