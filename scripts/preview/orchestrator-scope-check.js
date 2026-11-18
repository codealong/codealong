// Checks orchestrators listed together under All projects read apart: each is called Orchestrator,
// so each row's chip says what it keeps watch over, a project or All projects, with the compass
// (LaneRow.tsx `PlaceChip`); a view with one lists it with no chip, as its section says it.
//   scripts/preview.sh sessions '' scripts/preview/orchestrator-scope-check.js
(function () {
  // The first project's (the stub's), and All projects' on another session.
  window.__ORCHESTRATES__ = { project: '$first' };
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var second = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (second ? s.key !== second : (l.hidden || s.visitor || s.archived || s.orchestrator || s.helper || s.key === window.__ORCHESTRATOR_KEY__)) return;
          second = s.key;
          s.orchestrator = { schedule: 'orch-all', lookedAt: Date.now() - 60e3 };
          s.title = 'Orchestrator';
        });
      });
      return board;
    });
  };
  var rowOf = function (key) { return key && document.querySelector('.ca-orchestrators [id$="::' + key + '"]'); };
  var chip = function (row) { var c = row && row.querySelector('.ca-wtchip'); return c ? c.textContent.trim() : null; };
  say('orchestrator scope check: waiting for both');
  until(function () { return rowOf(window.__ORCHESTRATOR_KEY__) && rowOf(second); }, 20000, 'two orchestrator rows')
    .then(function () {
      var a = rowOf(window.__ORCHESTRATOR_KEY__);
      var b = rowOf(second);
      var title = function (row) { return row.querySelector('.ca-agentline-title').textContent.trim(); };
      check('both are called Orchestrator', title(a) === 'Orchestrator' && title(b) === 'Orchestrator', title(a) + ' / ' + title(b));
      check('the project’s says its project, with the compass', !!chip(a) && chip(a) !== 'All projects' && !!a.querySelector('.ca-wtchip .ca-icon'), chip(a));
      check('All projects’ says so', chip(b) === 'All projects', chip(b));
      done();
    })
    .catch(stopped);
})();
