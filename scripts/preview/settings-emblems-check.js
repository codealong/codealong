// Checks Settings › Appearance's session emblems (`[emblems]`): None, Emoji and Drawn, each saying
// what it does; Emoji with a box for Jev's key while the keychain has none (the check's has none
// until one is saved), and under a search that shows it beside Settings › Jev's, a key saved in one
// shows in the other; Drawn offered with an account that draws (Codex), and once picked, a box for
// how they're drawn that shows Codealong's own style until you write yours and saves yours as you
// leave it. config.toml reads back as it was saved, as the engine's would.
//   scripts/preview.sh sessions '' scripts/preview/settings-emblems-check.js
(function () {
  var latest = null;
  var jevKey = '';
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    if (cmd === 'jev_key') return Promise.resolve(jevKey !== '');
    if (cmd === 'set_jev_key') {
      window.__calls.push([cmd, args]);
      jevKey = args.key.trim();
      return Promise.resolve(null);
    }
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
    (await until(function () { return q('.ca-usagebar-settings'); }, 20000, 'the settings button')).click();
    (await until(function () { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === 'Appearance'; })[0]; }, 20000, 'Appearance in Settings')).click();
    var field = await until(function () { return named('Session emblems'); }, 20000, 'Session emblems in Appearance');
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
    check('Emoji says Jev picks one, and needs its key', /needs a TypeSafe key for Jev/.test(named('Session emblems').textContent));
    // The keychain has no key yet, so its field asks for one here, as Settings › Jev does.
    check('with no key yet, a box for it', !!(named('Jev’s key') && named('Jev’s key').querySelector('input[type="password"]')));
    check('no box for how they’re drawn', !named('How they’re drawn'));
    // A search that finds both key fields shows General's and Jev's at once: a key saved in one
    // shows in the other.
    var find = q('.ca-settings-find input');
    type(find, 'TypeSafe');
    var both = await until(function () {
      var here = named('Jev’s key');
      var jev = named('API key');
      return here && jev && here.querySelector('input[type="password"]') && jev.querySelector('input[type="password"]') && [here, jev];
    }, 5000, 'a search for TypeSafe didn’t show both key fields');
    var keyBox = both[0].querySelector('input[type="password"]');
    keyBox.focus();
    type(keyBox, 'ts-made-up-key');
    key(keyBox, { key: 'Enter' });
    await until(function () { return calls('set_jev_key').length === 1; }, 5000, 'Return didn’t save the key');
    await until(function () { return /Saved in the keychain/.test(named('API key').textContent); }, 5000, 'Settings › Jev’s field didn’t hear the key was saved');
    check('a key saved under Session emblems shows in Settings › Jev’s field too', !named('API key').querySelector('input[type="password"]') && /Saved in the keychain/.test(named('Jev’s key').textContent));
    type(find, '');
    await until(function () { return named('Session emblems'); }, 5000, 'clearing the search didn’t bring General back');
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
