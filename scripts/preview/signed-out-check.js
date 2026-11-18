// Checks sessions whose account's sign-in was turned down (Session.signedOut, claude/transcript.rs
// `signed_out_of`) in the Sessions view. One Codealong carries on once its account is signed in
// again (`signedOut.continues`): its row says Signed out, its panel's header says what Claude Code
// said and that Codealong carries it on, and offers Sign in, which asks the engine to sign its
// account in (`sign_in`). One Codealong won't carry on says to send it something after. `/login` in
// a session's box signs its account in too, sending the session nothing. And a notice of an account
// signed out (board.rs `signed_out_notices`) offers Sign in. The sessions are real ones of this Mac,
// since a board rarely has one signed out, and the notice is put on the board.
//   scripts/preview.sh sessions '' scripts/preview/signed-out-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var at = Date.now() - 5 * 60e3;
  var said = 'OAuth token revoked · Please run /login';
  var plan = [
    { title: 'Signed out A', continues: true },
    { title: 'Signed out B', continues: false },
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
            picked.push({ key: s.key, lane: l.id, account: s.account });
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
          s.stoppable = false;
          s.steerable = false;
          s.limit = undefined;
          s.cutOff = undefined;
          s.apiError = undefined;
          s.connectionLost = undefined;
          s.excerpt = said;
          s.signedOut = { at: at, said: said, continues: p.continues };
        });
      });
      if (picked.length) {
        var account = picked[0].account;
        board.notices = (board.notices || []).filter(function (n) { return n.key.indexOf('signed-out:') !== 0; });
        board.notices.push({ key: 'signed-out:' + account, title: 'Signed out of ' + account, text: '2 sessions stopped because ' + account + "'s sign-in was turned down. Sign in, and Codealong tells them to carry on.", signIn: account });
      }
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var flags = function (i) { return all('.ca-insight', row(i)).map(function (c) { return c.textContent; }); };
  var note = function () { return all('.ca-sessinsp-head .ca-sessinsp-limit').filter(function (n) { return /sign-in was turned down/.test(n.textContent); })[0]; };
  var words = function () { return note().querySelector('.ca-sessinsp-limit-words').textContent; };
  var button = function () { return note().querySelector('button'); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea') || document.querySelector('.ca-composer textarea'); };
  var noteText = function () { var n = document.querySelector('.ca-composer-note'); return n ? n.textContent : ''; };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title && note(); }, 3000, plan[i].title + ' shown with its note').then(function () { return wait(50); });
  };

  say('Signed out check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(1); }, 20000, 'two unpinned Claude Code sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      check('a turn its sign-in was turned down for says so in the list', flags(0).indexOf('Signed out') >= 0, flags(0).join(', '));
      check('the other too', flags(1).indexOf('Signed out') >= 0, flags(1).join(', '));
      return show(0);
    })
    .then(function () {
      var account = picked[0].account;
      check('its header says what Claude Code said, and that Codealong carries it on', words() === 'Its last turn ended because ' + account + '’s sign-in was turned down: ' + said + '. Codealong tells it to carry on once the account is signed in again, which often comes right by itself in a minute or two.', words());
      check('and offers Sign in', !!button() && button().textContent === 'Sign in', button() ? button().textContent : 'no button');
      button().click();
      return until(function () { return calls('sign_in').length > 0; }, 3000, 'Sign in asking the engine');
    })
    .then(function () {
      check('Sign in signs its account in', calls('sign_in')[0][1].name === picked[0].account, JSON.stringify(calls('sign_in')[0][1]));
      return show(1);
    })
    .then(function () {
      check('one Codealong won’t carry on says to send it something after', / Once you’ve signed in, send it something to go on\.$/.test(words()), words());
      return until(box, 3000, 'its box');
    })
    .then(function () {
      var sends = calls('send_to_session').length;
      var signs = calls('sign_in').length;
      type(box(), '/login');
      return wait(50).then(function () {
        enter();
        return until(function () { return calls('sign_in').length > signs; }, 3000, '/login asking the engine');
      }).then(function () {
        var last = calls('sign_in')[calls('sign_in').length - 1][1];
        check('/login in its box signs its account in', last.name === picked[1].account, JSON.stringify(last));
        check('and sends the session nothing', calls('send_to_session').length === sends);
        check('clears the box', box().value === '', box().value);
        return until(function () { return noteText().indexOf('Signing in to ' + picked[1].account) === 0; }, 3000, 'the box saying so');
      });
    })
    .then(function () {
      var chip = document.querySelector('.ca-notice-open');
      check('the notice of an account signed out shows', !!chip, chip ? chip.textContent : 'none');
      chip.click();
      return until(function () { return all('.ca-notice-item').some(function (n) { return /^Signed out of /.test(n.querySelector('.ca-notice-title').textContent); }); }, 3000, 'its card');
    })
    .then(function () {
      var item = all('.ca-notice-item').filter(function (n) { return /^Signed out of /.test(n.querySelector('.ca-notice-title').textContent); })[0];
      var offered = all('button', item).map(function (b) { return b.textContent; });
      check('it offers Sign in', offered.indexOf('Sign in') >= 0, offered.join(', '));
      var signs = calls('sign_in').length;
      all('button', item).filter(function (b) { return b.textContent === 'Sign in'; })[0].click();
      return until(function () { return calls('sign_in').length > signs; }, 3000, 'the notice’s Sign in asking the engine');
    })
    .then(function () {
      var last = calls('sign_in')[calls('sign_in').length - 1][1];
      check('which signs that account in', last.name === picked[0].account, JSON.stringify(last));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
