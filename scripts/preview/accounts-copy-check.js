// Checks Settings › Accounts' New account form taking another account's settings
// (ui/AccountsSection.tsx `copySources`): a new Claude Code account offers the accounts whose
// harness copies settings (`copiesSettings`) and whose folder is there, the first picked, and
// None; adding it saves it, copies from the one picked, and only then puts Codealong in it. A copy
// that fails says so and leaves Codealong out, for adding it again to copy the rest. A Codex
// account, whose harness copies none, isn't offered any, and None copies nothing. The accounts
// are made up here, so the check doesn't hang on this Mac's.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/accounts-copy-check.js
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: null, start: null, acp: null, monthly: null, per_token: false, disabled: false, makesPages: false, copiesSettings: false, folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { command: 'claude', email: 'ana@example.com', makesPages: true, copiesSettings: true }),
    info('claude2', 'claude', '~/.claude2', { command: 'claude2', email: 'ana@work.example', makesPages: true, copiesSettings: true }),
    info('claude-old', 'claude', '~/.claude-old', { command: 'claude-old', makesPages: true, copiesSettings: true, folderExists: false }),
    info('codex', 'codex', '~/.codex', { command: 'codex' }),
  ];
  // Whether the next copy fails, as one that can't read a file does.
  var failCopy = false;
  // The order the page asked for what adding an account does.
  var order = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    if (cmd === 'chrome_profiles') return Promise.resolve([]);
    if (cmd === 'save_accounts' || cmd === 'copy_account_settings' || cmd === 'install_agents') order.push(cmd);
    if (cmd === 'copy_account_settings' && failCopy) {
      failCopy = false;
      return Promise.reject('reading ~/.claude/settings.json: Permission denied');
    }
    // What's saved is what's read back, as config.toml would have it.
    if (cmd === 'save_accounts') {
      accounts = args.accounts.map(function (a) { return Object.assign({ makesPages: a.harness === 'claude', copiesSettings: a.harness === 'claude', folderExists: true, signedIn: false, email: null, signInNote: null }, accounts.filter(function (b) { return b.name === a.name; })[0], a); });
    }
    return real(cmd, args, options);
  };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (label) { return all('.ca-acct-form .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent.trim() === label; })[0]; };
  var choices = function (f) { return all('.ca-segctl button', f).map(function (b) { return b.textContent.trim() + (b.classList.contains('is-on') ? '*' : ''); }).join(' | '); };
  var nameField = function () { return q('.ca-acct-form input'); };

  (async function () {
    try {
      await until(function () { return q('.ca-side-config'); }, 20000, 'no settings button');
      q('.ca-side-config').click();
      await until(function () { return button('Accounts', q('.ca-settings-nav')); }, 5000, 'Settings didn’t open');
      button('Accounts', q('.ca-settings-nav')).click();
      await until(function () { return button('Add Claude Code account'); }, 5000, 'Settings › Accounts offered no new account');

      button('Add Codex account').click();
      await until(function () { return q('.ca-acct-form'); }, 3000, 'Add showed no form');
      check('a Codex account is offered no settings to copy', !field('Settings from'));
      button('Cancel', q('.ca-acct-form')).click();

      await until(function () { return button('Add Claude Code account'); }, 3000, 'the form didn’t close');
      button('Add Claude Code account').click();
      await until(function () { return field('Settings from'); }, 3000, 'a Claude Code account was offered no settings to copy');
      check('it offers the Claude Code accounts, the first picked, then None', choices(field('Settings from')) === 'claude* | claude2 | None', choices(field('Settings from')));
      check('it says what goes and what stays', /claude goes into the new folder/.test(field('Settings from').textContent) && /sign-in \(an API key too\), sessions and history stay with it/.test(field('Settings from').textContent), field('Settings from').textContent);
      button('claude2', field('Settings from')).click();
      await wait(100);
      check('another can be picked', choices(field('Settings from')) === 'claude | claude2* | None', choices(field('Settings from')));
      var name = nameField().value;
      button('Add account', q('.ca-acct-form')).click();
      await until(function () { return calls('install_agents').length > 0; }, 3000, 'adding it didn’t put Codealong in it');
      check('it copies from the one picked into the new account', JSON.stringify(last('copy_account_settings')) === JSON.stringify({ from: 'claude2', to: name }), JSON.stringify(last('copy_account_settings')));
      check('it saves, then copies, then puts Codealong in', order.join(',') === 'save_accounts,copy_account_settings,install_agents', order.join(','));

      await until(function () { return button('Add Claude Code account'); }, 3000, 'the form didn’t close after adding');
      button('Add Claude Code account').click();
      await until(function () { return field('Settings from'); }, 3000, 'the next account was offered no settings');
      check('the account just added is offered too, and one whose folder isn’t there isn’t', choices(field('Settings from')).indexOf(name) >= 0 && choices(field('Settings from')).indexOf('claude-old') < 0, choices(field('Settings from')));
      button('None', field('Settings from')).click();
      await wait(100);
      check('None says the folder starts empty', /starts with an empty folder/.test(field('Settings from').textContent), field('Settings from').textContent);
      button('Add account', q('.ca-acct-form')).click();
      await until(function () { return calls('install_agents').length > 1; }, 3000, 'the second account wasn’t added');
      check('None copies nothing', calls('copy_account_settings').length === 1, String(calls('copy_account_settings').length));

      await until(function () { return button('Add Claude Code account'); }, 3000, 'the form didn’t close after the second');
      button('Add Claude Code account').click();
      await until(function () { return field('Settings from'); }, 3000, 'the third account was offered no settings');
      failCopy = true;
      var installs = calls('install_agents').length;
      button('Add account', q('.ca-acct-form')).click();
      await until(function () { return q('.ca-form-error'); }, 3000, 'a failed copy said nothing');
      var said = q('.ca-form-error').textContent;
      check('a failed copy says what didn’t copy and how to copy the rest', /not all of claude’s settings copied/.test(said) && /Permission denied/.test(said) && /add it again/.test(said), said);
      await wait(300);
      check('and leaves Codealong out of it', calls('install_agents').length === installs, String(calls('install_agents').length - installs));

      button('Edit', document.getElementById('ca-acct-claude2')).click();
      await until(function () { return q('.ca-acct-form'); }, 3000, 'Edit showed no form');
      check('editing an account offers no settings to copy', !field('Settings from'));
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
