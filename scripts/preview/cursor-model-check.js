// Checks the model a Cursor app chat is set to, beside Send in its box (SessionReply.tsx,
// SessionModel.tsx): Cursor's name for it and, quieter, its effort as Cursor writes the level,
// between turns too, since the chat keeps the model its next turn goes on with; and no menu,
// since nothing switches a Cursor chat's model from outside.
//   scripts/preview.sh peek '' scripts/preview/cursor-model-check.js
(function () {
  // This runs before the app's script: pick a Cursor app chat on this Mac that says its model.
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
      if (!picked && s.harness === 'cursor' && s.surface === 'app' && s.model && s.status !== 'needs' && !s.archived && !l.hidden && !l.device) picked = s;
    });
  });
  if (!picked) { stopped('no Cursor app chat that says its model'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var said = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-said'); };
  var effort = function () { var d = q('.ca-modelmenu-detail', said()); return d ? d.textContent : ''; };
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };

  say('cursor model check: ' + picked.key + ' on ' + picked.model + (picked.effort ? ' ' + picked.effort : ''));
  // Between turns, as a chat the app isn't generating is.
  window.__emit('board', withSession(function (s) { s.live = false; s.status = 'idle'; }));
  until(function () { return !!said(); }, 20000, 'no model beside Send for a chat between turns')
    .then(function () {
      check('it says the model by Cursor’s name for it', said().textContent.indexOf(picked.model) === 0, said().textContent);
      if (picked.effort) check('after the model, its effort as Cursor writes the level', effort() === picked.effort, effort() || 'none');
      check('it’s no menu: nothing switches a Cursor chat’s model from here', !q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'), 'a menu');
      check('the chip sits beside Send', !!q('.ca-composer-send', said().closest('.ca-composer-buttons')), 'apart');
      // A turn under way says it too.
      window.__emit('board', withSession(function (s) { s.live = true; s.status = 'working'; s.effort = 'xhigh'; }));
      return until(function () { return effort() === 'xhigh'; }, 3000, 'the effort didn’t follow the chat: ' + effort());
    })
    .then(function () {
      // A chat whose model says no level shows the model alone.
      window.__emit('board', withSession(function (s) { s.live = false; s.status = 'idle'; s.model = 'Auto'; delete s.effort; }));
      return until(function () { return said() && said().textContent === 'Auto'; }, 3000, 'Auto with no effort: ' + (said() ? said().textContent : 'none'));
    })
    .then(done, stopped);
})();
