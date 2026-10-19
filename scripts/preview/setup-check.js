// Checks Set up Codealong (ui/SetupDialog.tsx) on a new Mac's accounts and folders
// (setup-sample.json): it starts with the listed accounts ticked and an idle one not, the folders
// searched or worked in ticked, a typed folder added ticked; Done saves, in one go, the accounts kept and the
// one added, the folders in order, the apps, puts Codealong in the accounts, opens at login when
// ticked and says setup is finished. Skip setup saves nothing and says it's finished too. A first
// run (`setup_needed`) opens it by itself. config.toml's accounts are the sample's listed ones,
// so the check doesn't hang on this Mac's.
//   PREVIEW_WAIT=30000 PREVIEW_SETUP=scripts/preview/setup-sample.json scripts/preview.sh sessions '' scripts/preview/setup-check.js
(function () {
  window.__setupNeeded = true;
  var listed = [
    { name: 'claude', harness: 'claude', home: '~/.claude', command: null },
    { name: 'codex', harness: 'codex', home: '~/.codex', command: null, monthly: 20 },
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_config') {
      return real(cmd, args, options).then(function (view) {
        view.config.accounts = JSON.parse(JSON.stringify(listed));
        view.config.scan = ['~/code'];
        return view;
      });
    }
    return real(cmd, args, options);
  };
  var dialog = function () { return q('.ca-setup'); };
  var rows = function () { return all('.ca-setup-row', dialog()); };
  var row = function (text) { return rows().filter(function (r) { return r.textContent.indexOf(text) >= 0; })[0]; };
  var ticked = function (text) { var r = row(text); return !!r && r.querySelector('input').checked; };
  var button = function (label) { return all('button', dialog()).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var heading = function () { return q('.ca-dialog-head h2', dialog()).textContent.trim(); };
  var setValue = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  (async function () {
    try {
      await until(function () { return dialog() && row('claude-work'); }, 20000, 'a first run didn’t open the wizard');
      check('a first run opens it by itself', true);
      check('it starts with the agents', heading() === 'Your agents', heading());
      check('the listed accounts start ticked', ticked('~/.claude') && ticked('~/.codex'));
      check('ones neither used nor signed in don’t', !ticked('claude-work') && !ticked('~/.cursor'));
      check('a row says where it stands', /starts with claude-work from ~\/\.zshrc/.test(row('claude-work').textContent), row('claude-work').textContent);
      row('claude-work').querySelector('input').click();
      row('~/.codex').querySelector('input').click();
      button('Next').click();

      await until(function () { return heading() === 'Your repos'; }, 5000, 'Next didn’t go on to the folders');
      check('the folder searched and the one worked in start ticked', ticked('~/code') && ticked('~/work') && !ticked('~/projects'));
      check('a folder says what it holds', /12 repos; your agents worked in 5 lately/.test(row('~/code').textContent), row('~/code').textContent);
      var input = q('.ca-setup-add input', dialog());
      input.focus();
      setValue(input, '~/src/');
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await until(function () { return row('~/src'); }, 5000, 'Return didn’t add the typed folder');
      check('a folder typed in is added, ticked, as config.toml writes it', ticked('~/src') && !row('~/src/'));
      check('and Return there stays on the step', heading() === 'Your repos', heading());
      button('Next').click();

      await until(function () { return heading() === 'Your apps'; }, 5000, 'Next didn’t go on to the apps');
      check('it asks for the terminal and the editor', all('select', dialog()).length === 2);
      button('Back').click();
      await until(function () { return heading() === 'Your repos'; }, 5000, 'Back didn’t go back');
      check('Back keeps what was picked', ticked('~/src') && !ticked('~/projects'));
      button('Next').click();
      await until(function () { return heading() === 'Your apps'; }, 5000);
      button('Next').click();

      await until(function () { return heading() === 'Ready'; }, 5000, 'Next didn’t go on to the last step');
      check('putting Codealong in the accounts starts ticked, opening at login not', ticked('Let your agents use Codealong') && !ticked('Open Codealong when you log in'));
      check('it says the quick panel’s shortcut', !!q('.ca-setup-tip .ca-kbds', dialog()));
      row('Open Codealong when you log in').querySelector('input').click();
      check('nothing is saved before Done', calls('save_accounts').length === 0 && calls('save_config').length === 0 && calls('finish_setup').length === 0);
      button('Done').click();

      await until(function () { return !dialog(); }, 5000, 'Done didn’t close the wizard');
      // One save: saving the settings writes the accounts too, so a separate one would be undone.
      check('Done saves once', calls('save_config').length === 1 && calls('save_accounts').length === 0, calls('save_config').length + ' + ' + calls('save_accounts').length);
      var cfg = last('save_config');
      var names = cfg ? cfg.config.accounts.map(function (a) { return a.name; }) : [];
      check('with the accounts kept and the one added', names.join(',') === 'claude,claude-work', names.join(','));
      check('a kept account as config.toml has it', cfg && cfg.config.accounts[0].home === '~/.claude');
      check('and the folders, in order', cfg && cfg.config.scan.join(',') === '~/code,~/work,~/src', cfg && cfg.config.scan.join(','));
      var put = last('install_agents');
      check('it puts Codealong in those accounts', put && put.accounts.join(',') === 'claude,claude-work', JSON.stringify(put));
      check('opens at login when ticked', (last('set_open_at_login') || {}).on === true);
      check('and says setup is finished', calls('finish_setup').length === 1);

      var before = window.__calls.length;
      window.__emit('setup', null);
      await until(function () { return dialog() && row('claude-work'); }, 5000, 'the app menu didn’t open it again');
      button('Skip setup').click();
      await until(function () { return !dialog(); }, 5000, 'Skip setup didn’t close it');
      var after = window.__calls.slice(before).map(function (c) { return c[0]; });
      check('Skip setup saves nothing', after.indexOf('save_accounts') < 0 && after.indexOf('save_config') < 0, after.join(','));
      check('and says setup is finished', after.indexOf('finish_setup') >= 0);
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
