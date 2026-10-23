// Checks a search in the quick panel with a project picked (quick.ts `findSessions`): it still
// finds a session in another project, whose row says that project before its worktree, and ⏎
// shows it in Codealong; the picked project stays as it was.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-other-project-check.js
(function () {
  // This runs before the app's script: the panel opens on one project, and a session in another
  // gets a title with a word nothing else has.
  var req = new XMLHttpRequest();
  req.open('GET', './board.json', false);
  req.send();
  var lanes = JSON.parse(req.responseText).lanes.filter(function (l) { return !l.hidden && !l.device && l.sessions.some(function (s) { return !s.visitor && !s.archived; }); });
  var home = lanes[0] ? lanes[0].project : null;
  // A worktree named as its project says the project once, by its name (ProjectSaid.tsx).
  var away = lanes.filter(function (l) { return l.project !== home && l.callsign.toLowerCase() !== l.project.toLowerCase(); })[0];
  if (home) localStorage.setItem('codealong.quick.project', home);
  var WORD = 'q' + Math.random().toString(36).slice(2, 10) + 'xz';
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The stub finds two sessions by what was said in them for any words; nothing here does.
    if (cmd === 'search_text') return Promise.resolve({ hits: [] });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board' || !away) return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        if (l.id !== away.id) return;
        l.sessions.forEach(function (s) {
          if (picked ? picked.key !== s.key : s.visitor || s.archived) return;
          s.title = 'Look into the ' + WORD + ' notes';
          picked = { laneId: l.id, key: s.key };
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row'); };
  var found = function () { return rows().filter(function (r) { return r.textContent.indexOf(WORD) >= 0; })[0]; };
  var projectShown = function () { var b = q('.ca-quick-project'); return b ? b.textContent : ''; };

  say('quick other project check: waiting for the list');
  if (!away) {
    check('sessions in two projects, one in a worktree not named as its project', false, home ? 'none beside ' + home : 'no session');
    done();
    return;
  }
  until(function () { return !!picked && !!input() && rows().length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      check('the panel shows the first project', projectShown().indexOf(home) >= 0, projectShown());
      type(input(), WORD);
      return until(function () { return !!found(); }, 5000).catch(function () {});
    })
    .then(function () {
      var row = found();
      check('a session in another project is found', !!row, rows().map(function (r) { return r.textContent.slice(0, 40); }).join(' | '));
      if (!row) return;
      var said = row.querySelector('.ca-projprefix-name');
      check('its row says its project', !!said && said.textContent === away.project, said ? said.textContent : row.textContent);
      check('the project picked stays', projectShown().indexOf(home) >= 0, projectShown());
      key(input(), { key: 'Enter' });
      return wait(300).then(function () {
        var shown = sent('show_main').pop();
        check('⏎ shows it in Codealong', !!shown && !!shown.reveal && shown.reveal.key === picked.key && shown.reveal.laneId === picked.laneId, JSON.stringify(shown));
      });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
