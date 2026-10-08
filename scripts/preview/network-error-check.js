// Checks a session whose turn a lost connection ended (Session.connectionLost,
// claude/transcript.rs `connection_lost_at`) in the Sessions view: its row says Network error, and
// that Codealong carries it on, in place of Jev's Blocked, which read Claude Code's error; and that
// Settings › Accounts has the setting that turns carrying on off (`[sessions]
// continue_after_network_error`), on by default, saved as config.toml has it. The loss is put on a
// real Claude Code session, since a board rarely has one.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/network-error-check.js
(function () {
  var key = null;
  var title = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!key && !l.hidden && !s.archived && !s.visitor && s.harness === 'claude' && s.status !== 'working') {
            key = s.key;
            title = s.title;
          }
          if (s.key !== key) return;
          s.status = 'unread';
          s.excerpt = 'API Error: Can’t reach the API server — check your internet or DNS (ENOTFOUND)';
          s.connectionLost = { at: Date.now() - 60000, continues: true };
          s.insight = Object.assign({}, s.insight, { landing: 'blocked' });
        });
      });
      return b;
    });
  };

  var rowOf = function () {
    return all('.ca-agentline-text').filter(function (t) { return t.querySelector('.ca-agentline-title') && t.querySelector('.ca-agentline-title').textContent === title; })[0];
  };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var box = function () {
    return all('.ca-settings-check').filter(function (l) { return /Carry on after a network error/.test(l.textContent); })[0];
  };

  (async function () {
    try {
      await until(function () { return key && rowOf(); }, 20000, 'no Claude Code session row to put a lost connection on');
      var text = rowOf();
      var chips = all('.ca-insight', text.closest('.ca-agentline') || text.parentElement).map(function (c) { return c.textContent; });
      var flag = all('.ca-insight', text).filter(function (c) { return c.textContent === 'Network error'; })[0];
      check('its row says Network error', !!flag, chips.join(', '));
      check('and that Codealong carries it on', flag && /tells it to carry on once the API can be reached again/.test(flag.title), flag ? flag.title : 'no chip');
      check('not Jev’s Blocked, which read the error', chips.indexOf('Blocked') < 0, chips.join(', '));

      q('.ca-side-config').click();
      await until(function () { return !!nav('Accounts'); }, 3000, 'no Accounts in Settings');
      nav('Accounts').click();
      await until(function () { return !!box(); }, 5000, 'no carry-on setting in Settings › Accounts');
      var input = box().querySelector('input');
      check('Settings › Accounts has it, on by default', input.checked, String(input.checked));
      input.click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'unchecking it saved nothing');
      var cfg = calls('save_config')[0][1].config;
      check('turned off, it saves [sessions] as config.toml has it', cfg && cfg.sessions && cfg.sessions.continue_after_network_error === false, JSON.stringify(cfg && cfg.sessions));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
