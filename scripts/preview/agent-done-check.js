// Checks that the main window says when an agent marks a session done from the command
// (agentdone.rs, server.rs, Toasts.tsx): the toast names the session that did it and what was
// done, and its Undo takes the session out of the archive.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/agent-done-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.archived) picked = { key: s.key, title: s.title };
        });
      });
      return board;
    });
  };

  var toast = function () { var t = document.querySelector('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var MESSAGE = function () { return '“Orchestrator” marked “' + picked.title + '” done: its terminal is closed and it’s archived.'; };

  say('agent done check: waiting for the list');
  until(function () { return !!picked && document.querySelectorAll('[role="listbox"] [id^="row-"]').length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      check('no toast before', toast() === '', toast());
      window.__emit('agent-done', { key: picked.key, title: picked.title, message: MESSAGE() });
      return until(function () { return toast() !== ''; }, 3000, 'the toast');
    })
    .then(function () {
      check('the toast says which session did it, and what was done', toast().indexOf(MESSAGE()) === 0, toast());
      var undo = document.querySelector('.ca-toast .ca-toast-action');
      check('with an Undo', !!undo && /Undo/.test(undo.textContent));
      undo.click();
      return wait(200);
    })
    .then(function () {
      var un = window.__calls.filter(function (c) { return c[0] === 'archive_session'; }).map(function (c) { return c[1]; });
      check('Undo takes it out of the archive', un.length === 1 && un[0].key === picked.key && un[0].archived === false, JSON.stringify(un));
      check('and the toast goes', toast() === '', toast());
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
