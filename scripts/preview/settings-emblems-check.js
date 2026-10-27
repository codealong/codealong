// Checks Settings › General's session emblems (`[emblems]`): None, Emoji and Drawn, each saying
// what it does; Drawn offered with an account that draws (Codex), and once picked, a box for how
// they're drawn that shows Codealong's own style until you write yours and saves yours as you
// leave it. config.toml reads back as it was saved, as the engine's would.
//   scripts/preview.sh sessions '' scripts/preview/settings-emblems-check.js
(function () {
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) { return latest ? Object.assign({}, view, { config: latest }) : view; });
  };
  var named = function (name) {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === name; })[0];
  };
  var choice = function (label) {
    return Array.prototype.filter.call(named('Session emblems').querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0];
  };
  var run = async function () {
    (await until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')).click();
    var field = await until(function () { return named('Session emblems'); }, 20000, 'Session emblems in General');
    var cfg = (await window.__TAURI_INTERNALS__.invoke('get_config', {})).config;
    var draws = (cfg.accounts || []).some(function (a) { return a.harness === 'codex' && !a.disabled; });
    check('None, Emoji and Drawn are offered', !!choice('None') && !!choice('Emoji') && (!draws || !!choice('Drawn')), field.textContent.slice(0, 120));
    // From None, whatever this Mac's config.toml has on.
    if ((cfg.emblems || {}).kind !== 'off') {
      choice('None').click();
      await until(function () { return latest && latest.emblems.kind === 'off'; }, 5000, 'None saved');
      await until(function () { return /only their titles/.test(named('Session emblems').textContent); }, 5000, 'what None does');
    }
    choice('Emoji').click();
    await until(function () { return latest && latest.emblems && latest.emblems.kind === 'emoji'; }, 5000, 'Emoji saved');
    await until(function () { return /Jev picks an emoji/.test(named('Session emblems').textContent); }, 5000, 'what Emoji does');
    check('Emoji says Jev picks one, and needs its key', /Settings › Jev/.test(named('Session emblems').textContent));
    check('no box for how they’re drawn', !named('How they’re drawn'));
    if (!draws) {
      check('without an account that draws, Drawn isn’t offered, and it says what would', !choice('Drawn') && /add one in Settings › Accounts/.test(named('Session emblems').textContent));
      return done();
    }
    choice('Drawn').click();
    await until(function () { return latest.emblems.kind === 'drawn'; }, 5000, 'Drawn saved');
    var box = await until(function () { var f = named('How they’re drawn'); return f && f.querySelector('textarea'); }, 5000, 'the box for how they’re drawn');
    check('Drawn says what each costs', /image turn/.test(named('Session emblems').textContent));
    check('the box shows Codealong’s own style until you write yours', box.value === '' && /plain white background/.test(box.placeholder), box.placeholder);
    var saves = calls('save_config').length;
    box.focus();
    var set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    set.call(box, '  Pixel art on a 16 by 16 grid.  ');
    box.dispatchEvent(new Event('input', { bubbles: true }));
    check('nothing is saved while you write', calls('save_config').length === saves);
    box.blur();
    await until(function () { return latest.emblems.prompt === 'Pixel art on a 16 by 16 grid.'; }, 5000, 'the prompt saved');
    check('leaving the box saves what you wrote, trimmed', latest.emblems.kind === 'drawn', JSON.stringify(latest.emblems));
    check('it says which account draws them', new RegExp('Said to ' + (latest.emblems.account || cfg.accounts.filter(function (a) { return a.harness === 'codex'; })[0].name)).test(named('How they’re drawn').textContent));
    done();
  };
  run().catch(stopped);
})();
