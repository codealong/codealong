// Checks the model and effort a Codex app thread is on, beside Send in its box (SessionReply.tsx,
// SessionModel.tsx): between turns too, since the app starts its next turn on them; and no menu,
// since nothing switches a thread in the app from outside. A thread of the terminal's says them
// only while it runs: between turns a send resumes it, on whatever that picks.
//   scripts/preview.sh peek '' scripts/preview/codex-model-check.js
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
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };

  say('codex model check: ' + picked.key + ' on ' + picked.model + (picked.effort ? ' ' + picked.effort : ''));
  // Between turns, as a thread no turn runs in is.
  window.__emit('board', withSession(function (s) { s.live = false; s.status = 'idle'; s.effort = 'xhigh'; }));
  until(function () { return !!said() && effort() === 'xhigh'; }, 20000, 'no model and effort beside Send for a thread between turns')
    .then(function () {
      check('it says the model the thread is on', said().textContent.indexOf(picked.model) === 0, said().textContent);
      check('it’s no menu: nothing switches a thread in the app from here', !q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'), 'a menu');
      // The same thread made in a terminal: between turns a send resumes it, so it says nothing.
      // The stub's way for it comes from here rather than its board.json (fake-tauri.js).
      var cli = function (s) { s.live = false; s.status = 'idle'; s.surface = 'cli'; };
      window.__madeSessions = window.__madeSessions || {};
      window.__madeSessions[picked.key] = JSON.parse(JSON.stringify(picked));
      cli(window.__madeSessions[picked.key]);
      window.__emit('board', withSession(cli));
      return until(function () { return !said(); }, 5000, 'a terminal thread between turns still says its model');
    })
    .then(done, stopped);
})();
