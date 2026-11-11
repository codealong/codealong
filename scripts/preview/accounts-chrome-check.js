// Checks an account's Chrome profile in Settings › Accounts (ui/AccountsSection.tsx): an account
// whose harness makes pages (Claude Code's artifacts) offers one when edited, listing Chrome's
// profiles, and saving it writes `chrome_profile`; its card then says where its pages open. An
// account whose harness makes none isn't offered one. The accounts and profiles are made up here,
// so the check doesn't hang on this Mac's.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/accounts-chrome-check.js
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: null, start: null, acp: null, monthly: null, per_token: false, disabled: false, makesPages: false, folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { command: 'claude', email: 'ana@example.com', makesPages: true }),
    info('claude2', 'claude', '~/.claude2', { command: 'claude2', email: 'ana@work.example', makesPages: true }),
    info('codex', 'codex', '~/.codex', { command: 'codex' }),
  ];
  var profiles = [
    { id: 'Personal', name: 'Personal', dir: 'Default', email: 'ana@example.com' },
    { id: 'Work', name: 'Work', dir: 'Profile 3', email: 'ana@work.example' },
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    if (cmd === 'chrome_profiles') return Promise.resolve(JSON.parse(JSON.stringify(profiles)));
    // What's saved is what's read back, as config.toml would have it.
    if (cmd === 'save_accounts') {
      accounts = args.accounts.map(function (a) { return Object.assign({}, accounts.filter(function (b) { return b.name === a.name; })[0], a); });
    }
    return real(cmd, args, options);
  };
  var card = function (name) { return document.getElementById('ca-acct-' + name); };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (label) { return all('.ca-acct-form .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent.trim() === label; })[0]; };
  var shown = function (name) {
    var dts = all('.ca-kv dt', card(name));
    var at = dts.filter(function (d) { return d.textContent.trim() === 'Pages open in'; })[0];
    return at ? at.nextElementSibling.textContent.trim() : null;
  };

  (async function () {
    try {
      await until(function () { return q('.ca-side-config'); }, 20000, 'no settings button');
      q('.ca-side-config').click();
      await until(function () { return button('Accounts', q('.ca-settings-nav')); }, 5000, 'Settings didn’t open');
      button('Accounts', q('.ca-settings-nav')).click();
      await until(function () { return card('codex'); }, 5000, 'Settings › Accounts listed no accounts');
      check('an account with none set says nothing of it', shown('claude2') === null);

      button('Edit', card('codex')).click();
      await until(function () { return q('.ca-acct-form'); }, 3000, 'Edit showed no form');
      check('an account whose harness makes no pages isn’t offered a profile', !field('Chrome profile'));
      button('Cancel', q('.ca-acct-form')).click();

      await until(function () { return card('claude2') && button('Edit', card('claude2')); }, 3000, 'the form didn’t close');
      button('Edit', card('claude2')).click();
      await until(function () { return field('Chrome profile'); }, 3000, 'an account that makes pages wasn’t offered a profile');
      var select = field('Chrome profile').querySelector('select');
      var options = all('option', select).map(function (o) { return o.textContent; });
      check('it lists none, then Chrome’s profiles', options.join(' | ') === 'None: the default browser | Personal · ana@example.com | Work · ana@work.example', options.join(' | '));
      var setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
      setter.call(select, 'Work');
      select.dispatchEvent(new Event('change', { bubbles: true }));
      await wait(100);
      button('Save', q('.ca-acct-form')).click();
      await until(function () { return calls('save_accounts').length > 0; }, 3000, 'Save saved nothing');
      var saved = calls('save_accounts')[0][1].accounts;
      var written = saved.map(function (a) { return a.name + '=' + a.chrome_profile; }).join(',');
      check('saving writes its profile and leaves the others as they were', written === 'claude=null,claude2=Work,codex=null', written);
      await until(function () { return shown('claude2'); }, 3000, 'the card didn’t say where its pages open');
      check('its card says where its pages open', shown('claude2') === 'Chrome’s Work profile', shown('claude2'));
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
