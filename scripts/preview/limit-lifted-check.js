// Checks the setting that carries a session on once its usage limit lifts (`[limits]
// continue_when_lifted`, reconnect.rs): Settings › Automation has it under the choice of what's
// done when a session hits its limit, on by default; turned off, it saves [limits] as config.toml
// has it, keeping that choice; and picking another choice keeps it as it was.
//   scripts/preview.sh sessions '' scripts/preview/limit-lifted-check.js
(function () {
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var box = function () { return all('.ca-settings-check').filter(function (l) { return /Carry on when the limit lifts/.test(l.textContent); })[0]; };
  var saved = function (n) { return calls('save_config')[n][1].config.limits; };

  (async function () {
    try {
      await until(function () { return !!q('.ca-side-config'); }, 20000, 'no Settings in the sidebar');
      q('.ca-side-config').click();
      await until(function () { return !!nav('Automation'); }, 3000, 'no Automation in Settings');
      nav('Automation').click();
      await until(function () { return !!box(); }, 5000, 'no setting to carry on when the limit lifts in Settings › Automation');
      var input = box().querySelector('input');
      check('Settings › Automation has it, on by default', input.checked, String(input.checked));
      check('it says Claude Code may carry on by itself first', /Claude Code may carry on by itself first/.test(box().textContent), box().textContent);
      input.click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'unchecking it saved nothing');
      check('turned off, it saves [limits] as config.toml has it', saved(0).continue_when_lifted === false, JSON.stringify(saved(0)));
      check('and keeps what’s done when a session hits its limit', saved(0).when_hit === 'suggest', JSON.stringify(saved(0)));
      var move = all('.ca-field').filter(function (f) { return /When a session hits its usage limit/.test(f.textContent); })[0];
      var pick = move && all('button', move).filter(function (b) { return b.getAttribute('aria-pressed') !== 'true' && b.getAttribute('aria-checked') !== 'true'; })[0];
      if (!pick) return stopped('no other choice of what’s done when a session hits its limit');
      pick.click();
      await until(function () { return calls('save_config').length > 1; }, 3000, 'picking another choice saved nothing');
      check('picking another choice keeps carrying on as the file has it', saved(1).when_hit !== 'suggest' && 'continue_when_lifted' in saved(1), JSON.stringify(saved(1)));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
