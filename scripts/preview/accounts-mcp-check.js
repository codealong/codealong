// Checks the MCP servers in Settings › Accounts (ui/AccountsSection.tsx `McpServers`): a few
// chips, then a `+N` that shows the rest and Fewer that folds them again; one more than the few
// shows whole rather than as `+1`; each chip says where it comes from and its tools on hover; an
// account with none says so, one whose read missed some says why, and one not read yet has no
// row. The accounts and their servers are made up here, so the check doesn't hang on this Mac's.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/accounts-mcp-check.js
(function () {
  var info = function (name, harness, home) {
    return { name: name, harness: harness, home: home, command: null, start: null, acp: null, monthly: null, per_token: false, disabled: false, folderExists: true, signedIn: true, email: null, signInNote: null };
  };
  var accounts = [info('claude', 'claude', '~/.claude'), info('claude2', 'claude', '~/.claude2'), info('codex', 'codex', '~/.codex'), info('cursor', 'cursor', '~/.cursor'), info('opencode', 'opencode', '~/.local/share/opencode')];
  var server = function (name, from, tools) { return { name: name, from: from, tools: tools || [] }; };
  var mcp = [
    { account: 'claude', asOf: Date.now(), servers: [server('Claude Docs', 'claude.ai connector'), server('Gmail', 'claude.ai connector', ['search_threads', 'get_thread', 'create_draft']), server('Google Calendar', 'claude.ai connector', ['list_events']), server('Google Drive', 'claude.ai connector'), server('Linear', 'claude.ai connector'), server('Slack', 'claude.ai connector'), server('figma', 'figma plugin'), server('northwind', '~/.claude.json')] },
    { account: 'claude2', asOf: Date.now(), servers: [server('Claude Docs', 'claude.ai connector')], note: 'Couldn’t read its claude.ai connectors: sign-in expired, run `claude2` once to sign in again' },
    { account: 'codex', asOf: Date.now(), servers: [server('GitHub', 'ChatGPT app'), server('Gmail', 'ChatGPT app'), server('Google Calendar', 'ChatGPT app'), server('atlas', '~/.codex/config.toml')] },
    { account: 'cursor', asOf: Date.now(), servers: [] },
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    if (cmd === 'get_board') return real(cmd, args, options).then(function (b) { return Object.assign(b, { mcp: mcp }); });
    return real(cmd, args, options);
  };
  var card = function (name) { return document.getElementById('ca-acct-' + name); };
  var row = function (name) { return q('.ca-acct-mcp', card(name)); };
  var chips = function (name) { return all('.ca-chip', row(name)).map(function (c) { return c.textContent.trim(); }); };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };

  (async function () {
    try {
      await until(function () { return q('.ca-side-config'); }, 20000, 'no settings button');
      q('.ca-side-config').click();
      await until(function () { return button('Accounts', q('.ca-settings-nav')); }, 5000, 'Settings didn’t open');
      button('Accounts', q('.ca-settings-nav')).click();
      await until(function () { return card('claude') && row('claude'); }, 5000, 'Settings › Accounts showed no MCP servers');

      check('the first three, then +5', chips('claude').join(',') === 'Claude Docs,Gmail,Google Calendar,+5', chips('claude').join(','));
      check('under its label', row('claude').previousElementSibling.textContent.trim() === 'MCP servers', row('claude').previousElementSibling.textContent);
      var gmail = all('.ca-chip', row('claude'))[1];
      check('a chip says where it comes from and its tools', gmail.title === 'claude.ai connector · 3 tools', gmail.title);
      check('one without tools listed just where', all('.ca-chip', row('claude'))[0].title === 'claude.ai connector', all('.ca-chip', row('claude'))[0].title);
      button('+5', row('claude')).click();
      await until(function () { return chips('claude').length === 9; }, 3000, '+5 showed nothing more');
      check('+5 shows them all, and Fewer', chips('claude').join(',') === 'Claude Docs,Gmail,Google Calendar,Google Drive,Linear,Slack,figma,northwind,Fewer', chips('claude').join(','));
      check('they wrap within the card', row('claude').getBoundingClientRect().right <= card('claude').getBoundingClientRect().right + 0.5);
      button('Fewer', row('claude')).click();
      await until(function () { return chips('claude').length === 4; }, 3000, 'Fewer didn’t fold them');
      check('Fewer folds them again', chips('claude').join(',') === 'Claude Docs,Gmail,Google Calendar,+5', chips('claude').join(','));

      check('four show whole, not three and +1', chips('codex').join(',') === 'GitHub,Gmail,Google Calendar,atlas', chips('codex').join(','));
      check('a read that missed some says why', /sign-in expired, run claude2 once/.test(q('.ca-acct-mcp-note', row('claude2')).textContent), row('claude2').textContent);
      check('an account with none says so', row('cursor').textContent.trim() === 'None', row('cursor').textContent);
      check('one not read yet has no row', !row('opencode'));
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
