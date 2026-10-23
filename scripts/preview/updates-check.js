// Checks the app menu's Check for updates… (App.tsx, ui/Updates.tsx `untilChecked`): the window
// it reaches says how the check goes in a toast and never opens Settings. A spinner says it's
// looking while the engine checks and installs; then the toast says a newer version is ready
// (and the sidebar offers the restart), that this is the latest, why it failed, or that a
// development build doesn't update. The engine's states are pushed as updates.rs would.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/updates-check.js
(function () {
  var settings = function () { return q('.ca-settings'); };
  var busy = function () { var t = q('.ca-toast.is-busy'); return t ? t.textContent : ''; };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var status = function (s) { window.__emit('update-status', (window.__update = Object.assign({ current: '0.5.0' }, s))); };
  // As the engine does: the check is marked as under way before the menu asks the window.
  var ask = function (s) {
    status(s);
    window.__emit('check-updates');
  };
  // A click puts a toast away; the next is waited for once it's gone.
  var dismiss = function () {
    var t = q('.ca-toast:not(.is-busy)');
    if (t) t.click();
    return until(function () { return !toast(); }, 3000, 'the toast to go');
  };

  say('updates check: waiting for the board');
  until(function () { return q('.ca-side'); }, 20000, 'the sidebar')
    .then(function () { return wait(500); })
    .then(function () {
      ask({ state: 'checking' });
      return until(function () { return busy(); }, 3000, 'the spinner');
    })
    .then(function () {
      check('a spinner says it’s looking', /Looking for updates/.test(busy()), busy());
      check('Settings stays shut', !settings());
      status({ state: 'installing', version: '0.6.0' });
      return wait(400);
    })
    .then(function () {
      check('it keeps looking while the update goes in', /Looking for updates/.test(busy()) && !toast(), busy() + ' / ' + toast());
      status({ state: 'ready', version: '0.6.0', notes: 'Faster' });
      return until(function () { return toast(); }, 3000, 'the toast');
    })
    .then(function () {
      check('the toast says the newer version is ready', toast() === 'Codealong 0.6.0 is ready: restart to update from the sidebar.', toast());
      check('the spinner goes', !busy(), busy());
      check('the sidebar offers the restart', !!q('.ca-side-update'), 'no Restart to update');
      check('Settings is still shut', !settings());
      return dismiss().then(function () {
        ask({ state: 'checking' });
        return wait(300);
      }).then(function () {
        status({ state: 'upToDate', checkedAt: Date.now() });
        return until(function () { return toast(); }, 3000, 'the toast');
      });
    })
    .then(function () {
      check('up to date says this is the latest', toast() === 'Codealong 0.5.0 is the latest version.', toast());
      return dismiss().then(function () {
        ask({ state: 'checking' });
        return wait(300);
      }).then(function () {
        status({ state: 'failed', error: 'Couldn’t check for updates: offline', at: Date.now() });
        return until(function () { return toast(); }, 3000, 'the toast');
      });
    })
    .then(function () {
      check('a failed check says why', toast() === 'Couldn’t check for updates: offline', toast());
      // A development build: the engine starts no check, and says so as it stands.
      return dismiss().then(function () {
        ask({ state: 'off' });
        return until(function () { return toast(); }, 3000, 'the toast');
      });
    })
    .then(function () {
      check('a development build says it never updates', /development build/.test(toast()), toast());
      check('and Settings never opened', !settings());
    })
    .then(done)
    .catch(stopped);
})();
