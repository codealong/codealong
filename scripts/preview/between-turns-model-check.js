// Checks the model and effort a session says beside Send in its box between turns (SessionReply.tsx,
// SessionModel.tsx), where its next turn goes on with them: a Codex app thread, since the app
// starts its next turn on them, and an OpenCode session, since its service runs a prompt that
// names no model on the session's; and no menu, since nothing switches either from outside. A
// Codex thread of the terminal's says them only while it runs: between turns a send resumes it,
// on whatever that picks.
//   scripts/preview.sh peek '' scripts/preview/between-turns-model-check.js
(function () {
  // This runs before the app's script: pick a Codex app thread on this Mac that says its model.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && s.harness === 'codex' && s.surface === 'app' && s.model && !s.hosted && s.status !== 'needs' && !s.archived && !l.hidden && !l.device) picked = s;
    });
  });
  if (!picked) { stopped('no Codex app thread that says its model'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var said = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-said'); };
  var effort = function () { var d = q('.ca-modelmenu-detail', said()); return d ? d.textContent : ''; };
  // The board the box gets, and the session the stub says the way for (fake-tauri.js answers
  // `session_way` from `__madeSessions` before its board.json); the way is asked again as the
  // session's surface changes.
  var become = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    window.__madeSessions = window.__madeSessions || {};
    window.__madeSessions[picked.key] = JSON.parse(JSON.stringify(picked));
    change(window.__madeSessions[picked.key]);
    window.__emit('board', b);
  };

  say('between turns model check: ' + picked.key + ' on ' + picked.model + (picked.effort ? ' ' + picked.effort : ''));
  // Between turns, as a thread no turn runs in is.
  become(function (s) { s.live = false; s.status = 'idle'; s.effort = 'xhigh'; });
  until(function () { return !!said() && effort() === 'xhigh'; }, 20000, 'no model and effort beside Send for a Codex app thread between turns')
    .then(function () {
      check('it says the model the thread is on', said().textContent.indexOf(picked.model) === 0, said().textContent);
      check('it’s no menu: nothing switches a thread in the app from here', !q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'), 'a menu');
      // The same thread made in a terminal: between turns a send resumes it, so it says nothing.
      become(function (s) { s.live = false; s.status = 'idle'; s.surface = 'cli'; });
      return until(function () { return !said(); }, 5000, 'a terminal thread between turns still says its model');
    })
    .then(function () {
      // An OpenCode session between turns, its model as OpenCode names it and its variant.
      become(function (s) { s.harness = 'opencode'; s.live = false; s.status = 'idle'; s.surface = 'app'; s.model = 'big-pickle'; s.effort = 'high'; });
      return until(function () { return !!said() && said().textContent.indexOf('big-pickle') === 0 && effort() === 'high'; }, 5000, 'an OpenCode session between turns: ' + (said() ? said().textContent : 'nothing'));
    })
    .then(function () {
      check('it’s no menu for an OpenCode session either', !q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'), 'a menu');
    })
    .then(done, stopped);
})();
