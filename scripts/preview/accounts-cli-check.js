// Checks Settings › Accounts (ui/AccountsSection.tsx) for an account whose command line isn't on
// this machine (harness.rs `missing_cli`): its card says which and why, and Install opens the
// quick panel on a chat with agentTasks.ts's prompt filled in (api.ts `askAgent`), as the
// iPhone app's button does, so you pick the agent that installs it. An account whose command line
// is here says nothing of it. The accounts are made up here, so the check doesn't hang on this
// Mac's.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/accounts-cli-check.js
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: null, start: null, acp: null, monthly: null, per_token: false, disabled: false, folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { command: 'claude', email: 'ana@example.com' }),
    info('cursor', 'cursor', '~/.cursor', { command: 'cursor-agent', email: 'ana@example.com', missingCli: 'cursor-agent' }),
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
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
      await until(function () { return card('cursor'); }, 5000, 'Settings › Accounts listed no accounts');

      check('the account without its command line is tagged so', tags('cursor').indexOf('No cursor-agent') >= 0, tags('cursor').join(','));
      var note = all('.ca-acct-note', card('cursor')).map(function (n) { return n.textContent.trim(); }).join(' | ');
      check('and says why it matters', /cursor-agent, Cursor’s command line, isn’t on this machine/.test(note), note);
      check('the one whose command line is here says nothing of it', !tags('claude').some(function (t) { return /^No /.test(t); }) && !button('Install claude…', card('claude')), tags('claude').join(','));

      var install = button('Install cursor-agent…', card('cursor'));
      check('it offers to install it', !!install);
      install.click();
      await until(function () { return calls('show_quick').length > 0; }, 3000, 'Install opened no quick panel');
      var open = calls('show_quick')[0][1].context;
      check('in a chat of its own', open.chat === true && !open.laneId, JSON.stringify(open));
      check('with the prompt filled in', /^Install cursor-agent, Cursor’s command line, on this machine\./.test(open.prompt || '') && /with `cursor-agent`/.test(open.prompt), open.prompt);
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
