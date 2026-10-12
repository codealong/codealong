// Checks that Allow shows the session working at once, before the engine has answered, and that
// an answer that doesn't go brings the call back, saying why (ahead.ts `answerAhead`,
// SessionReply.tsx `Approval`): one session waits on a call and nothing else waits. The engine's
// answers are held until the check lets them go.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/answer-ahead-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var boardNow = null;
  var asked = 0;
  // The engine's answer to each session_answer, for the check to give when it chooses.
  var held = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      asked += 1;
      return Promise.resolve({ id: 'p' + asked, label: 'Run `npm test`', input: 'npm test', cannot: null });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (resolve, reject) { held.push({ resolve: resolve, reject: reject }); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.lastActivity = s.statusSince;
          s.stalledSince = null;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Queue A';
          picked = { key: s.key, lane: l.id, since: s.statusSince };
        });
      });
      boardNow = board;
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var approvalNote = function () { var a = approval(); var n = a && a.querySelector('.ca-composer-note'); return n ? n.textContent : ''; };
  var button = function (text) { var a = approval(); return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.indexOf(text) === 0; })[0]; };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var rowSays = function (status) { var r = row(); return !!(r && r.querySelector('.ca-glyph--' + status)); };
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }).map(function (c) { return c[1]; }); };
  // The board as the engine has it, the session's status changed.
  var push = function (status, since) {
    var b = JSON.parse(JSON.stringify(boardNow));
    b.generatedAt = Date.now();
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) { s.status = status; s.statusSince = since; } }); });
    boardNow = b;
    window.__emit('board', b);
  };

  say('answer ahead check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Queue A' && button('Allow'); }, 5000);
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('the row says it needs you', rowSays('needs'));
      button('Allow').click();
      return until(function () { return answers().length === 1 && !approval(); }, 3000).catch(function () {});
    })
    .then(function () {
      var a = answers()[0];
      check('Allow asks the engine', a && a.key === picked.key && a.answer === 'allow' && a.id === 'p1', JSON.stringify(a));
      check('the call goes from the panel before the engine has answered', !approval() && held.length === 1);
      check('and the row shows it working', rowSays('working'));
      // The engine answers, then a board that still shows the call comes: it stays working.
      held[0].resolve('Allowed Bash(npm test).');
      return wait(200).then(function () { push('needs', picked.since); return wait(300); });
    })
    .then(function () {
      check('a board from before it took the answer leaves it working', rowSays('working') && !approval());
      // It asks again: a new call, which needs you.
      push('needs', Date.now());
      return until(function () { return asked >= 2 && button('Allow'); }, 5000).then(function () { return wait(100); });
    })
    .then(function () {
      check('a call asked after it needs you again', rowSays('needs'));
      button('Allow').click();
      return until(function () { return answers().length === 2 && !approval(); }, 3000).then(function () { return wait(100); });
    })
    .then(function () {
      held[1].reject('It stopped waiting before the answer reached it. Look where it runs.');
      return until(function () { return button('Allow'); }, 3000).then(function () { return wait(200); });
    })
    .then(function () {
      check('an answer that didn’t go brings the call back', !!button('Allow') && rowSays('needs'));
      check('saying why', /stopped waiting before the answer reached it/.test(approvalNote()), approvalNote());
      // A plain deny ends the turn, which only the board can say how it shows.
      button('Deny').click();
      return until(function () { return answers().length === 3; }, 3000).then(function () { return wait(200); });
    })
    .then(function () {
      check('Deny leaves the call shown until the engine answers', !!approval() && rowSays('needs'), approvalNote());
      held[2].resolve('Denied Bash(npm test); it stops and waits for you.');
      return wait(300);
    })
    .then(function () {
      // Answered, the box says so in place of the call.
      var said = document.querySelector('.ca-sessinsp .ca-composer.is-waiting');
      check('then says what it did', !!said && /^Denied/.test(said.textContent), said ? said.textContent : 'no box');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
