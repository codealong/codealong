// Checks a link to a session or a todo that macOS opens with Codealong, a click on one in a
// terminal or another app (front.rs `open_links`): the main window takes the link as it comes up,
// which is how a click that launched the app arrives, and shows the session it names once the
// board is read; told of another while it's up (a `link` reveal), it takes that one and shows its
// todo; and one that names nothing it lists says so and leaves what's shown alone. The session is
// one of this Mac's board, the todo one the check adds.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/opened-link-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  var TODO = 't-openedlink001';
  var picked = null;
  var read = null;
  var board = new Promise(function (r) { read = r; });
  var first = true;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The link that launched the app is there before the page asks for it, which it does as it
    // comes up, before the board has come: the session's key is known once that has.
    if (cmd === 'take_opened_link' && first) {
      first = false;
      window.__calls.push([cmd, args, options]);
      return board.then(function () { return picked ? 'codealong://session/' + picked.key : null; });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      // Not the first row, which the window shows by itself.
      var seen = 0;
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (l.hidden || s.visitor || s.archived || s.origin) return;
          if (seen++ >= 3 && !picked) picked = { key: s.key, lane: l.id, title: s.title };
        });
      });
      var now = Date.now();
      b.todos = (b.todos || []).filter(function (t) { return t.id !== TODO; }).concat([
        { id: TODO, project: '', title: 'Open the link check', description: '', status: 'todo', labels: [], worktrees: [], sessions: [], createdAt: now, updatedAt: now, doneAt: null, pending: [], dependsOn: [], waiting: null, review: null, due: null },
      ]);
      read();
      return b;
    });
  };

  var title = function () { var t = q('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var toast = function () { var t = q('.ca-toast'); return t ? t.textContent : ''; };

  say('opened link check: waiting for the board');
  board
    .then(function () {
      if (!picked) throw new Error('the board has no fourth session to link to');
      return until(function () { return title() === picked.title; }, 15000, 'the session the link names, in the panel');
    })
    .then(function () {
      check('a link that launched the app shows its session once the board is read', true);
      check('its row is the one selected', !!document.getElementById('row-' + picked.lane + '::' + picked.key));
      check('the link was asked for as the window came up', calls('take_opened_link').length === 1, calls('take_opened_link').length + '');
      // Another clicked while the app is up: the app keeps it and tells the window.
      window.__OPENED_LINK__ = 'codealong://todo/' + TODO;
      window.__emit('reveal', { kind: 'link' });
      return until(function () { return q('.ca-todoinsp-title'); }, 5000, 'the todo the second link names');
    })
    .then(function () {
      var t = q('.ca-todoinsp-title');
      check('told of another, the window takes it and shows its todo', (t.value || t.textContent) === 'Open the link check', t.value || t.textContent);
      check('each link is taken once', calls('take_opened_link').length === 2 && window.__OPENED_LINK__ === null, calls('take_opened_link').length + '');
      window.__OPENED_LINK__ = 'codealong://session/claude:claude:no-such-session-0000';
      window.__emit('reveal', { kind: 'link' });
      return until(function () { return /no session/.test(toast()); }, 5000, 'word that there’s no such session');
    })
    .then(function () {
      check('one that names no session says so', toast() === 'There’s no session claude:claude:no-such-session-0000 here.', toast());
      check('and leaves what’s shown alone', !!q('.ca-todoinsp-title'));
      window.__OPENED_LINK__ = 'codealong://link/cal1abc';
      window.__emit('reveal', { kind: 'link' });
      return until(function () { return /names nothing/.test(toast()); }, 5000, 'word that the link names nothing');
    })
    .then(function () {
      check('a link of another kind names nothing Codealong shows', toast() === 'That link names nothing Codealong shows.', toast());
    })
    .then(done)
    .catch(stopped);
})();
