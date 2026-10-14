// Checks Settings › Accounts (ui/AccountsSection.tsx) for an account turned off and one whose
// harness signs in to other providers: the one turned off is tagged so and offers Turn on; Turn
// off saves the account with `disabled` and leaves the others as they were; and OpenCode's card
// says what it's signed in to in place of an email, with no sign-in asked of it. The accounts are
// made up here, so the check doesn't hang on this Mac's.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/accounts-off-check.js
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: null, start: null, preferred: false, acp: null, monthly: null, per_token: false, disabled: false, folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { command: 'claude', email: 'ana@example.com' }),
    info('cursor', 'cursor', '~/.cursor', { command: 'cursor-agent', disabled: true }),
    info('opencode', 'opencode', '~/.local/share/opencode', { command: 'opencode', signInNote: 'Free models only' }),
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    // What's saved is what's read back, as config.toml would have it.
    if (cmd === 'save_accounts') {
      accounts = args.accounts.map(function (a) { return Object.assign({}, accounts.filter(function (b) { return b.name === a.name; })[0], a); });
    }
    return real(cmd, args, options);
  };
  var card = function (name) { return document.getElementById('ca-acct-' + name); };
  var tags = function (name) { return all('.ca-acct-tags .ca-tag', card(name)).map(function (t) { return t.textContent.trim(); }); };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };

  (async function () {
    try {
      await until(function () { return q('.ca-side-config'); }, 20000, 'no settings button');
      q('.ca-side-config').click();
      await until(function () { return button('Accounts', q('.ca-settings-nav')); }, 5000, 'Settings didn’t open');
      button('Accounts', q('.ca-settings-nav')).click();
      await until(function () { return card('opencode'); }, 5000, 'Settings › Accounts listed no accounts');

      check('OpenCode says what it runs on in place of an email', tags('opencode').join(',') === 'Free models only', tags('opencode').join(','));
      check('and is asked no sign-in', !all('button', card('opencode')).some(function (b) { return /^Sign in/.test(b.textContent.trim()); }));
      check('an account turned off is tagged so', card('cursor').classList.contains('is-off') && tags('cursor')[0] === 'Turned off', tags('cursor').join(','));
      check('and offers Turn on', !!button('Turn on', card('cursor')) && !button('Turn off', card('cursor')));
      check('one that’s on offers Turn off, and no tag says it’s on', !!button('Turn off', card('claude')) && tags('claude').indexOf('Turned off') < 0 && !card('claude').classList.contains('is-off'), tags('claude').join(','));

      button('Turn off', card('claude')).click();
      await until(function () { return calls('save_accounts').length > 0; }, 3000, 'Turn off saved nothing');
      var saved = calls('save_accounts')[0][1].accounts;
      var off = saved.map(function (a) { return a.name + '=' + a.disabled; }).join(',');
      check('Turn off saves it turned off, and the others as they were', off === 'claude=true,cursor=true,opencode=false', off);
      check('with what the form doesn’t show kept', saved[0].preferred === false && saved[0].command === 'claude' && saved[1].command === 'cursor-agent', JSON.stringify(saved[0]));
      await until(function () { return card('claude') && card('claude').classList.contains('is-off'); }, 3000, 'the card didn’t show it turned off');
      check('its card then shows it turned off', tags('claude')[0] === 'Turned off' && !!button('Turn on', card('claude')), tags('claude').join(','));

      button('Turn on', card('cursor')).click();
      await until(function () { return calls('save_accounts').length > 1; }, 3000, 'Turn on saved nothing');
      var on = calls('save_accounts')[1][1].accounts.map(function (a) { return a.name + '=' + a.disabled; }).join(',');
      check('Turn on saves it on again', on === 'claude=true,cursor=false,opencode=false', on);
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
