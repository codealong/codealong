// Checks the model a Cursor app chat is set to, after its account in the chip beside Send in its box
// (SessionReply.tsx, SessionAgent.tsx): Cursor's name for it and, quieter, its effort as Cursor writes the level,
// between turns too, since the chat keeps the model its next turn goes on with; and no menu,
// since nothing switches a Cursor chat's model from outside. Cursor's name for the model is the
// one its account lists it by (start-options.json), which the chip says once the window has read
// the account's models; for the moment before that it says the name the chat's record has.
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

  // The model as the chat's account names it (start.ts `modelName`): the label it lists it by,
  // up to a colon, or the record's own name for one it doesn't list.
  var account = read('./start-options.json').accounts.filter(function (a) { return a.name === picked.account && a.harness === picked.harness; })[0];
  var listed = ((account && account.models) || []).filter(function (m) { return m.id === picked.model; })[0];
  var named = listed ? listed.label.split(':')[0] : picked.model;

  var said = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-said'); };
  // After the account, quieter: `model · effort`.
  var detail = function () { var d = said() && q('.ca-modelmenu-detail', said()); return d ? d.textContent : ''; };
  var model = function () { return detail().split(' · ')[0]; };
  var effort = function () { return detail().split(' · ')[1] || ''; };
  // The level the chat is on between turns, as the check has it: not the one the board's chat
  // has, so the chip saying it is the board the check sent having been heard.
  var between = picked.effort === 'medium' ? 'low' : 'medium';
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };

  say('cursor model check: ' + picked.key + ' on ' + picked.model + (picked.effort ? ' ' + picked.effort : ''));
  until(function () { return !!said(); }, 20000, 'no model beside Send in the peek’s box')
    .then(function () {
      // Between turns, as a chat the app isn't generating is, and on a level of the check's own:
      // sent once the peek shows its box, which is when the window hears of a board (this runs
      // before the app's script, and a board sent as it starts is heard by nobody).
      window.__emit('board', withSession(function (s) { s.live = false; s.status = 'idle'; s.effort = between; }));
      // Waited for until the chip says that level, which only this board has, and names the
      // model as its account does, which comes with the account's models, read after the box
      // shows: how long after is the machine's to say. Read the moment the chip showed, it said
      // the record's name on a fast Mac and the account's on a slow one.
      return until(function () { return !!said() && effort() === between && model() === named; }, 5000).catch(function () {});
    })
    .then(function () {
      check('it says the model by Cursor’s name for it, after its account', said().textContent.indexOf(picked.account) === 0 && model() === named, said().textContent + ' (wanted ' + named + ')');
      check('after the model, its effort as Cursor writes the level, between turns too', effort() === between, (effort() || 'none') + ' (wanted ' + between + ')');
      check('it’s no menu: nothing switches a Cursor chat’s model from here', !q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'), 'a menu');
      check('the chip sits beside Send', !!q('.ca-composer-send', said().closest('.ca-composer-buttons')), 'apart');
      // A turn under way says it too.
      window.__emit('board', withSession(function (s) { s.live = true; s.status = 'working'; s.effort = 'xhigh'; }));
      return until(function () { return effort() === 'xhigh'; }, 3000, 'the effort didn’t follow the chat: ' + effort());
    })
    .then(function () {
      // A chat whose model says no level shows the model alone.
      window.__emit('board', withSession(function (s) { s.live = false; s.status = 'idle'; s.model = 'Auto'; delete s.effort; }));
      return until(function () { return said() && detail() === 'Auto'; }, 3000, 'Auto with no effort: ' + (said() ? said().textContent : 'none'));
    })
    .then(done, stopped);
})();
