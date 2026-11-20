// Checks Set up Codealong (ui/SetupDialog.tsx) on a new Mac's accounts and folders
// (setup-sample.json): it starts with the listed accounts ticked and an idle one not, the folders
// searched or worked in ticked, a typed folder added ticked; Done saves, in one go, the accounts kept and the
// one added, the folders in order, the apps and the emblems picked, puts Codealong in the
// accounts, opens at login when ticked and says setup is finished. Session emblems start off;
// Drawn is offered only while an account that draws (Codex) is kept, and Emoji always, with a box
// for Jev's key while there's none, whose Return saves the key and stays on the step. Skip setup
// saves nothing and says it's finished too. A first run (`setup_needed`) opens it by itself. What
// the last step suggests for later leads with pull request reviews, and Set up reviews finishes it
// and opens Settings at Automation, on its reviews. config.toml's accounts are the sample's listed
// ones, with emblems off, so the check doesn't hang on this Mac's or start from its emblems.
//   PREVIEW_WAIT=30000 PREVIEW_SETUP=scripts/preview/setup-sample.json scripts/preview.sh sessions '' scripts/preview/setup-check.js
(function () {
  window.__setupNeeded = true;
  var listed = [
    { name: 'claude', harness: 'claude', home: '~/.claude', command: null },
    { name: 'codex', harness: 'codex', home: '~/.codex', command: null, monthly: 20 },
  ];
  // The keychain, as the app has it: no key for Jev until one is saved.
  var jevKey = '';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_config') {
      return real(cmd, args, options).then(function (view) {
        view.config.accounts = JSON.parse(JSON.stringify(listed));
        view.config.scan = ['~/code'];
        view.config.emblems = { kind: 'off', account: '', prompt: '' };
        return view;
      });
    }
    if (cmd === 'decision_keys') {
      window.__calls.push([cmd, args]);
      return Promise.resolve({ typesafe: jevKey !== '' });
    }
    if (cmd === 'set_decision_key') {
      window.__calls.push([cmd, args]);
      jevKey = args.key.trim();
      return Promise.resolve(null);
    }
    return real(cmd, args, options);
  };
  var choice = function (label) { return all('.ca-segctl button', dialog()).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (name) { return all('.ca-field', dialog()).filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === name; })[0]; };
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

      await until(function () { return heading() === 'Terminal and editor'; }, 5000, 'Next didn’t go on to the terminal and editor');
      check('it asks for the terminal and the editor', all('select', dialog()).length === 2);
      button('Back').click();
      await until(function () { return heading() === 'Your repos'; }, 5000, 'Back didn’t go back');
      check('Back keeps what was picked', ticked('~/src') && !ticked('~/projects'));
      button('Next').click();
      await until(function () { return heading() === 'Terminal and editor'; }, 5000);
      button('Next').click();

      await until(function () { return heading() === 'Your sessions'; }, 5000, 'Next didn’t go on to session emblems');
      check('session emblems start off', choice('None') && choice('None').classList.contains('is-on'), choice('None') && choice('None').className);
      check('without an account kept that draws, Drawn isn’t offered, and it says what would', !choice('Drawn') && /A Codex account can draw them/.test(field('Session emblems').textContent), field('Session emblems').textContent);
      check('no box for Jev’s key before Emoji', !field('Jev’s key'));
      choice('Emoji').click();
      var keyBox = await until(function () { var f = field('Jev’s key'); return f && f.querySelector('input[type="password"]'); }, 5000, 'Emoji didn’t ask for Jev’s key');
      check('Emoji says a decision model picks one, and asks for Jev’s key while there’s none', /model picks an emoji/.test(field('Session emblems').textContent) && !!keyBox);
      keyBox.focus();
      setValue(keyBox, '  ts-made-up-key  ');
      keyBox.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await until(function () { return calls('set_decision_key').length === 1; }, 5000, 'Return didn’t save the key');
      check('Return saves the key to the keychain', last('set_decision_key').key.trim() === 'ts-made-up-key' && last('set_decision_key').provider === 'typesafe', JSON.stringify(last('set_decision_key')));
      await until(function () { var f = field('Jev’s key'); return f && /Saved in the keychain/.test(f.textContent); }, 5000, 'the key’s field didn’t say it was saved');
      check('and stays on the step, saying it’s saved', heading() === 'Your sessions', heading());
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
      check('and the emblems picked', cfg && cfg.config.emblems.kind === 'emoji', cfg && JSON.stringify(cfg.config.emblems));
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

      // What's worth setting up later: a button for each that finishes and opens it in Settings.
      window.__emit('setup', null);
      await until(function () { return dialog() && row('claude-work'); }, 5000, 'the app menu didn’t open it a third time');
      for (var i = 0; i < 3; i++) {
        button('Next').click();
        await wait(50);
      }
      // Codex is kept this time, so it can draw them.
      await until(function () { return heading() === 'Your sessions'; }, 5000, 'Next didn’t go on to session emblems again');
      check('with Codex kept, Drawn is offered', !!choice('Drawn'));
      choice('Drawn').click();
      await until(function () { var f = field('How they’re drawn'); return f && f.querySelector('textarea'); }, 5000, 'Drawn didn’t offer a box for how they’re drawn');
      check('Drawn says what each costs, and who draws them', /image turn/.test(field('Session emblems').textContent) && /Said to codex/.test(field('How they’re drawn').textContent), field('How they’re drawn').textContent.slice(0, 80));
      check('no box for Jev’s key for Drawn', !field('Jev’s key'));
      button('Next').click();
      await until(function () { return heading() === 'Ready'; }, 5000, 'Next didn’t go on to the last step again');
      var more = all('.ca-setup-more-row .ca-setup-label', dialog()).map(function (l) { return l.textContent.trim(); });
      check('the last step suggests pull request reviews first, then decision models, other devices and automation', more.join(',') === 'Pull request reviews,Decision models,Your other devices,Automation', more.join(','));
      var finished = calls('finish_setup').length;
      button('Set up reviews').click();
      await until(function () { return !dialog(); }, 5000, 'Set up reviews didn’t close the wizard');
      check('Set up reviews finishes setup', calls('finish_setup').length === finished + 1);
      var drawn = (last('save_config') || { config: {} }).config.emblems;
      check('saving drawn emblems, by the account kept that draws', drawn && drawn.kind === 'drawn' && drawn.account === 'codex', JSON.stringify(drawn));
      var reviews = await until(function () { var p = q('.ca-settings-pane'); return p && p.textContent.indexOf('Review pull requests that wait on your review') >= 0 && p; }, 5000, 'Settings didn’t open at its reviews');
      check('and opens Settings at Automation, on its reviews', !!reviews);
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
