// Checks a session whose turn a lost connection ended (Session.connectionLost,
// claude/transcript.rs `connection_lost_at`) in the Sessions view: its row says Network error, and
// that Codealong carries it on, in place of Jev's Blocked, which read Claude Code's error; and that
// Settings › Automation has the setting that turns carrying on off (`[sessions]
// continue_after_network_error`), on by default, saved as config.toml has it; and beside it the one
// that carries on a session Codealong runs after Codealong quitting cut it off
// (`continue_after_restart`, reconnect.rs) and the one that resumes a session for one it started
// (`resume_for_sessions_it_started`, send.rs `wakes`), on by default too, each saved without
// changing the others. The loss is put on a real Claude Code session, since a board rarely has one.
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
          // Not a headless run or a helper, which Sessions counts on another session's row.
          if (!key && !l.hidden && !s.archived && !s.visitor && !s.headless && !s.helper && s.harness === 'claude' && s.status !== 'working') {
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
  var labelled = function (re) {
    return function () { return all('.ca-settings-check').filter(function (l) { return re.test(l.textContent); })[0]; };
  };
  var box = labelled(/Carry on after a network error/);
  var restartBox = labelled(/Carry on after Codealong restarts/);
  var resumeBox = labelled(/Resume a session for one it started/);

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
      await until(function () { return !!nav('Automation'); }, 3000, 'no Automation in Settings');
      nav('Automation').click();
      await until(function () { return !!box(); }, 5000, 'no carry-on setting in Settings › Automation');
      var input = box().querySelector('input');
      check('Settings › Automation has it, on by default', input.checked, String(input.checked));
      input.click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'unchecking it saved nothing');
      var cfg = calls('save_config')[0][1].config;
      check('turned off, it saves [sessions] as config.toml has it', cfg && cfg.sessions && cfg.sessions.continue_after_network_error === false, JSON.stringify(cfg && cfg.sessions));
      check('and leaves resuming a session for one it started on', cfg && cfg.sessions && cfg.sessions.resume_for_sessions_it_started === true, JSON.stringify(cfg && cfg.sessions));
      check('and carrying on after a restart', cfg && cfg.sessions && cfg.sessions.continue_after_restart === true, JSON.stringify(cfg && cfg.sessions));
      await until(function () { return !!resumeBox(); }, 3000, 'no resume setting in Settings › Automation');
      var resume = resumeBox().querySelector('input');
      check('resuming a session for one it started is there, on by default', resume.checked, String(resume.checked));
      resume.click();
      await until(function () { return calls('save_config').length > 1; }, 3000, 'unchecking resuming saved nothing');
      var then = calls('save_config')[1][1].config;
      check('turned off, it saves resume_for_sessions_it_started = false', then && then.sessions && then.sessions.resume_for_sessions_it_started === false, JSON.stringify(then && then.sessions));
      await until(function () { return !!restartBox(); }, 3000, 'no restart setting in Settings › Automation');
      var restart = restartBox().querySelector('input');
      check('carrying on after Codealong restarts is there, on by default', restart.checked, String(restart.checked));
      restart.click();
      await until(function () { return calls('save_config').length > 2; }, 3000, 'unchecking carrying on after a restart saved nothing');
      var last = calls('save_config')[2][1].config;
      check('turned off, it saves continue_after_restart = false', last && last.sessions && last.sessions.continue_after_restart === false, JSON.stringify(last && last.sessions));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
