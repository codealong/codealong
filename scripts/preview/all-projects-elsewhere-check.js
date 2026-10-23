// Checks that All projects has Elsewhere's sessions (board.rs `hidden`, quick.ts `inProject`): a
// folder outside your projects holding a session you haven't archived is on the board, and under
// All projects that session is listed in Sessions with Elsewhere said before its worktree, as a
// session of any project is.
//   scripts/preview.sh sessions '' scripts/preview/all-projects-elsewhere-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  // This runs before the app's script: a session in an Elsewhere folder gets a title with a word
  // nothing else has and is made recent, so the age filter keeps it.
  var req = new XMLHttpRequest();
  req.open('GET', './board.json', false);
  req.send();
  var own = function (s) { return !s.visitor && !s.archived && !s.helper && !s.headless; };
  var folders = JSON.parse(req.responseText).lanes.filter(function (l) { return l.project === 'Elsewhere' && !l.device && !l.chats && !l.startsChats; });
  var lane = folders.filter(function (l) { return l.sessions.some(own); })[0];
  var WORD = 'q' + Math.random().toString(36).slice(2, 10) + 'xz';
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board' || !lane) return p;
    return p.then(function (board) {
      var now = Date.now();
      board.lanes.forEach(function (l) {
        if (l.id !== lane.id) return;
        l.lastActivity = now;
        l.sessions.forEach(function (s) {
          if (picked ? picked !== s.key : !own(s)) return;
          s.title = 'Look into the ' + WORD + ' notes';
          s.lastActivity = now;
          picked = s.key;
        });
      });
      return board;
    });
  };
  var rows = function () { return all('.ca-board-list [id^="row-"]'); };
  var found = function () { return rows().filter(function (r) { return r.textContent.indexOf(WORD) >= 0; })[0]; };

  say('all projects elsewhere check: waiting for the list');
  if (!lane) {
    check('a folder outside your projects with a session in it', false, folders.length + ' Elsewhere folders, none with a session of its own');
    done();
    return;
  }
  check('a folder outside your projects with a session in it is on the board', !lane.hidden, lane.callsign + ' is hidden');
  until(function () { return !!picked && rows().length > 0; }, 20000, 'the list')
    .then(function () { return until(function () { return !!found(); }, 5000).catch(function () {}); })
    .then(function () {
      var row = found();
      check('under All projects, its session is listed', !!row, rows().length + ' rows, none with the word');
      if (!row) return;
      var said = row.querySelector('.ca-projprefix-name');
      check('its row says Elsewhere before its worktree', !!said && said.textContent === 'Elsewhere', said ? said.textContent : row.textContent.slice(0, 80));
    })
    .then(done)
    .catch(stopped);
})();
