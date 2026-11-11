// Checks sessions whose last turn an API error ended (Session.apiError, claude/transcript.rs
// `api_error_of`) in the Sessions view. One stopped on "529 Overloaded": its row says API error, its
// panel's header says what the error said and offers Try again, which sends the session menus.ts
// `TRY_AGAIN` as a message of yours, as its box sends one. One on a refused connection, which
// Codealong carries on by itself once its API can be reached (Session.connectionLost.continues): its
// row says Network error, and its header says Codealong carries it on and offers Try now. Both are
// put on real sessions of this Mac, since a board rarely has one.
//   scripts/preview.sh sessions '' scripts/preview/api-error-check.js
(function () {
  // This runs before the app's script: the board it gets has the two stand-ins, made from real
  // sessions on this device that aren't pinned (which list apart).
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var at = Date.now() - 5 * 60e3;
  var plan = [
    { title: 'API error A', said: '529 Overloaded. This is a server-side issue, usually temporary.' },
    { title: 'API error B', said: 'Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)', lost: true },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      var pinned = board.pinned || [];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          var i = picked.findIndex(function (x) { return x.key === s.key && x.lane === l.id; });
          if (i < 0 && (l.hidden || l.device || s.visitor || s.archived || s.helper || s.headless || s.harness !== 'claude' || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length)) return;
          seen[s.key] = true;
          if (i < 0) {
            i = picked.length;
            picked.push({ key: s.key, lane: l.id });
          }
          var p = plan[i];
          s.title = p.title;
          s.status = 'unread';
          s.statusSince = at;
          s.lastActivity = at;
          s.live = true;
          s.asks = false;
          s.stalledSince = null;
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.quiet = false;
          s.limit = undefined;
          s.cutOff = undefined;
          s.excerpt = 'API Error: ' + p.said;
          s.apiError = { at: at, said: p.said };
          s.connectionLost = p.lost ? { at: at, continues: true } : undefined;
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var flags = function (i) { return all('.ca-insight', row(i)).map(function (c) { return c.textContent; }); };
  var errorNote = function () { return all('.ca-sessinsp-head .ca-sessinsp-limit').filter(function (n) { return /API error/.test(n.textContent); })[0]; };
  var words = function () { return errorNote().querySelector('.ca-sessinsp-limit-words').textContent; };
  var button = function () { return errorNote().querySelector('button'); };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title && errorNote(); }, 3000, plan[i].title + ' shown with its note').then(function () { return wait(50); });
  };

  say('API error check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(1); }, 20000, 'two unpinned Claude Code sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      check('a turn an API error ended says so in the list', flags(0).indexOf('API error') >= 0, flags(0).join(', '));
      check('a refused connection is a network error', flags(1).indexOf('Network error') >= 0, flags(1).join(', '));
      return show(0);
    })
    .then(function () {
      check('its header says what the error said', words() === 'Its last turn ended on an API error: ' + plan[0].said, words());
      check('and offers Try again', !!button() && button().textContent === 'Try again', button() ? button().textContent : 'no button');
      button().click();
      return until(function () { return calls('send_to_session').length > 0; }, 3000, 'Try again sending it something');
    })
    .then(function () {
      var sent = calls('send_to_session')[0][1];
      check('Try again sends it the message to try again, as its box sends one', sent.key === picked[0].key && /^Your last turn ended on an API error\. Try again/.test(sent.text) && !sent.now, JSON.stringify(sent));
      return show(1);
    })
    .then(function () {
      check('a refused connection’s header says Codealong carries it on', /^Its last turn ended on an API error: Connection refused .*\(ECONNREFUSED\)\. Codealong tells it to carry on once the API can be reached again\.$/.test(words()), words());
      check('and offers Try now', !!button() && button().textContent === 'Try now', button() ? button().textContent : 'no button');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
