// Checks a search Jev says fits none of the sessions it read (route.ts `suggestions`, Quick.tsx):
// nothing is suggested, the note says it looks like a search, Send to goes under the sessions
// found, and ⏎ does the first thing the search found rather than opening Send to: a session, and
// a PR when it finds only that.
//   PREVIEW_ROUTE=scripts/preview/route-search-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-search-check.js
// The board needs a session in a worktree that isn't hidden, and a PR.
(function () {
  // This runs before the app's script: one of the board's real sessions, and one of its PRs, get
  // a title with a word nothing else has, so a search for it finds that alone.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var word = function () { return 'q' + Math.random().toString(36).slice(2, 10) + 'xz'; };
  var SESSION = word() + ' notes';
  var PR = word() + ' draft';
  var picked = null;
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The stub finds two sessions by what was said in them for any words; nothing here does.
    if (cmd === 'search_text') return Promise.resolve({ hits: [] });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked ? picked.key !== s.key || picked.laneId !== l.id : l.hidden || s.visitor || s.archived) return;
          s.title = 'Pick the ' + SESSION;
          picked = { laneId: l.id, key: s.key };
        });
      });
      (board.prs || []).forEach(function (x) {
        if (pr ? pr !== x.key : false) return;
        x.title = 'Open the ' + PR;
        pr = x.key;
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row'); };
  var titled = function (text) { return rows().filter(function (r) { return r.textContent.indexOf(text) >= 0; })[0]; };
  var sendTo = function () { return rows().filter(function (r) { return /Send to (a|another) session/.test(r.textContent); })[0]; };
  var noteRow = function () { return q('.ca-quick-row.is-suggest.is-note'); };
  var activeRow = function () { return q('.ca-quick-row.is-active'); };
  // Types the words and waits for Jev's answer and the row they find.
  var search = function (words) {
    var asked = calls('route_quick').length;
    type(input(), words);
    return until(function () { return calls('route_quick').length > asked && !!noteRow() && !noteRow().classList.contains('is-routing') && !!titled(words); }, 10000).then(function () { return wait(200); });
  };

  say('quick search check: waiting for the list');
  until(function () { return !!picked && !!input() && rows().length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () { return search(SESSION); })
    .then(function () {
      check('nothing is suggested', all('.ca-quick-row.is-suggest:not(.is-note):not(.is-stale)').length === 0);
      check('the note says it looks like a search', /looks like a search/.test(noteRow().textContent), noteRow().textContent);
      var at = rows().indexOf(sendTo());
      check('Send to comes after the session found', !!sendTo() && at > rows().indexOf(titled(SESSION)), String(at));
      check('the session found is the row ⏎ does', activeRow() === titled(SESSION), (activeRow() || {}).textContent);
      key(input(), { key: 'Enter' });
      return wait(300);
    })
    .then(function () {
      var shown = sent('show_main').pop();
      check('⏎ shows the session found', !!shown && !!shown.reveal && shown.reveal.key === picked.key, JSON.stringify(shown));
      check('⏎ puts the panel away', calls('hide_quick').length > 0);
      if (!pr) throw new Error('the board has no PR to find');
      return search(PR);
    })
    .then(function () {
      check('with only a PR found, Send to comes first', rows().indexOf(sendTo()) >= 0 && rows().indexOf(sendTo()) < rows().indexOf(titled(PR)));
      check('the PR found is the row ⏎ does', activeRow() === titled(PR), (activeRow() || {}).textContent);
      key(input(), { key: 'Enter' });
      return wait(300);
    })
    .then(function () {
      var shown = sent('show_main').pop();
      check('⏎ shows the PR found', !!shown && !!shown.reveal && shown.reveal.kind === 'pr' && shown.reveal.key === pr, JSON.stringify(shown));
    })
    .then(done)
    .catch(stopped);
})();
