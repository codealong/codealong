// Checks the account-level usage limit banner (accountLimit.ts `accountLimitFor`, SessionHeader.tsx)
// in the bar's peek for a session whose own last turn finished normally, when its account's usage
// meter is at 100% (without usable extra usage): its header shows the warning banner naming the
// account, limit and reset time, offers Snooze until it lifts and Move…, and hides the banner once
// extra usage is usable or the session is snoozed (removing the snooze button).
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/account-limit-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : null;
  };

  var board = get('./board.json');
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && !l.hidden && !l.device && !s.archived && !s.visitor && s.harness === 'claude' && s.resumeCommand) picked = s;
    });
  });
  if (!picked) { stopped('no Claude Code session to check the account limit banner on'); return; }
  var sk = picked.key;
  var acct = picked.account;
  var lifts = Date.now() + 9 * 3600 * 1000 + 5 * 60 * 1000;
  localStorage.setItem('codealong.preview.peek', sk);

  var shape = function (b, opts) {
    opts = opts || {};
    b.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        if (s.account === acct) s.limit = undefined;
        if (s.key !== sk) return;
        s.status = 'idle';
        s.limit = undefined;
        s.cutOff = undefined;
        s.signedOut = undefined;
        s.apiError = undefined;
        s.connectionLost = undefined;
        s.snoozedUntil = opts.snoozed ? lifts : undefined;
        s.snoozedForRoom = undefined;
      });
    });
    (b.usage || []).forEach(function (u) {
      if (u.account !== acct) return;
      u.needsSignIn = false;
      u.loading = false;
      u.noLimits = false;
      u.extraUsable = opts.extraUsable;
      u.meters = [
        { key: 'session', label: '5h', window: '5-hour session', percent: 45, resetsAt: Date.now() + 2 * 3600 * 1000, severity: 'normal', model: null },
        { key: 'week', label: 'Week', window: 'Weekly', percent: 100, resetsAt: lifts, severity: 'critical', model: null },
      ];
    });
    return b;
  };

  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) { return shape(b); });
  };

  var banner = function () { return q('.ca-sessinsp-head .ca-sessinsp-limit.is-warn'); };

  (async function () {
    try {
      var line = await until(banner, 20000, 'the account-level limit banner');
      var words = q('.ca-sessinsp-limit-words', line);
      check('the banner names the account, limit and reset time', words && words.textContent.indexOf(acct + ' at weekly limit · lifts in 9h') === 0, words ? words.textContent : 'no words');
      check('its tooltip explains that starting a new turn will hit the limit', words && words.title.indexOf('Starting a new turn will hit it') >= 0, words ? words.title : 'no title');
      var snooze = all('button', line).filter(function (b) { return b.textContent === 'Snooze until it lifts'; })[0];
      var move = all('button', line).filter(function (b) { return b.textContent === 'Move…'; })[0];
      check('it offers Snooze until it lifts and Move…', !!snooze && !!move, all('button', line).map(function (b) { return b.textContent; }).join(' | '));
      snooze.click();
      await until(function () { return calls('snooze_sessions').length > 0; }, 3000, 'snooze_sessions call');
      var asked = calls('snooze_sessions')[0][1];
      check('Snooze until it lifts snoozes until the account limit resets', asked.keys.join() === sk && asked.until === lifts && !asked.room, JSON.stringify(asked));

      // Once extra usage is usable, the 100% meter no longer shows the warning banner.
      var b = shape(JSON.parse(JSON.stringify(board)), { extraUsable: true });
      window.__emit('board', b);
      await until(function () { return !banner(); }, 5000, 'banner gone when extra usage is usable');
      check('the banner clears when extra usage can carry turns past 100%', !banner(), 'still shown');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
