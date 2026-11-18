// Provider flags show in session rows and the shared header. The Automation setting defaults
// on even in older config files, and saves an explicit opt-out without removing the badge.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/flagged-sessions-check.js
//   PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/flagged-sessions-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'desktop_feed') return p.then(function (f) {
      f.lanes.forEach(function (l) { l.sessions.forEach(function (s) { s.flagged = true; }); });
      return f;
    });
    if (cmd === 'get_config') return p.then(function (view) {
      var cfg = Object.assign({}, view.config);
      cfg.sessions = Object.assign({}, cfg.sessions);
      delete cfg.sessions.block_flagged_sessions;
      return Object.assign({}, view, { config: cfg });
    });
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || s.visitor || s.archived || s.helper || s.headless || l.hidden || l.device) return;
          picked = { key: s.key, lane: l.id };
          s.flagged = true;
        });
      });
      return b;
    });
  };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var chips = function (el) { return all('.ca-insight', el).filter(function (c) { return c.textContent === 'Flagged'; }); };
  var setting = function () { return all('.ca-settings-check').find(function (f) { return (q(':scope > span:not(.ca-checkbox)', f) || {}).textContent === 'Block automations for flagged sessions'; }); };
  (async function () {
    try {
      if (window.__CODEALONG_DESKTOP__) {
        var badge = await until(function () { return q('.ca-cv-card .ca-insight'); }, 20000, 'desktop card flag');
        check('the shared card face shows the flag', badge.textContent === 'Flagged');
        var r = badge.getBoundingClientRect();
        var line = badge.closest('.ca-cv-l2').getBoundingClientRect();
        check('the card does not clip the badge', r.top >= line.top && r.bottom <= line.bottom && r.left >= line.left && r.right <= line.right, JSON.stringify([r.top, r.bottom, line.top, line.bottom]));
        return done();
      }
      await until(row, 20000, 'a session row');
      check('the row shows its provider flag', chips(row()).length === 1);
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      await until(function () { return q('.ca-sessinsp') && chips(q('.ca-sessinsp')).length; }, 10000, 'the shared header flag');
      var chip = chips(q('.ca-sessinsp'))[0];
      check('the flag explains the automation guard and manual replies', /blocked by default/.test(chip.title) && /reply yourself/.test(chip.title), chip.title);
      var rect = chip.getBoundingClientRect();
      check('the badge fits in the window', rect.left >= 0 && rect.right <= innerWidth && rect.width > 0);
      q('.ca-side-config').click();
      var automation = await until(function () { return all('.ca-settings-nav button').find(function (b) { return b.textContent.trim() === 'Automation'; }); }, 5000, 'Automation tab');
      automation.click();
      await until(setting, 5000, 'flagged session setting');
      var input = q('input', setting());
      check('older settings default to blocking', input.checked);
      var before = calls('save_config').length;
      input.click();
      await until(function () { return calls('save_config').length > before; }, 3000, 'setting saved');
      var cfg = calls('save_config').slice(-1)[0][1].config;
      check('opting out saves false', cfg.sessions.block_flagged_sessions === false);
      check('the explanation says the flag stays', /stays across later turns/.test(setting().textContent));
      done();
    } catch (e) { stopped(e); }
  })();
})();
