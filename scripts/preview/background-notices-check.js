// Checks that a window's own background failures are notices in the accounts bar, and end once
// what failed works again (notices.ts `reportNotice`, `clearNotice`): where each account can
// start a session, read at launch for Review with (App.tsx), and Jev's suggestions for a todo
// (TodoPanel.tsx). Both fail at first and the notices show; then they work, Jev's on the next
// todo and the start options as a PR's menu asks for them, and the notices go.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-jev-sample.json scripts/preview.sh todo '' scripts/preview/background-notices-check.js
(function () {
  // This runs before the page's own scripts, so the app asks through this from the start.
  var failing = { start_options: true, suggest_todo: true };
  var tauri = window.__TAURI_INTERNALS__;
  var invoke = tauri.invoke;
  tauri.invoke = function (cmd) {
    if (failing[cmd]) return Promise.reject('the engine is busy');
    return invoke.apply(this, arguments);
  };
  // The notices' titles and texts, from their card: it opens, is read, and closes again, each
  // step once the page shows the one before.
  var read = function () {
    var chip = q('.ca-notice-open');
    if (!chip) return Promise.resolve({ titles: [], texts: [] });
    if (!q('.ca-notice-card')) chip.click();
    return until(function () { return q('.ca-notice-card'); }, 3000, 'the notices’ card').then(function (card) {
      var got = {
        titles: all('.ca-notice-title', card).map(function (el) { return el.textContent; }),
        texts: all('.ca-notice-text', card).map(function (el) { return el.textContent; }),
      };
      q('.ca-notice-open').click();
      return until(function () { return !q('.ca-notice-card'); }, 3000, 'the card to close').then(function () { return got; });
    });
  };
  // Resolves once `test` holds of what the notices say, asking again until it does.
  var once = function (test, what) {
    var start = Date.now();
    return new Promise(function (res, rej) {
      (function poll() {
        read().then(function (got) {
          if (test(got)) return res(got);
          if (Date.now() - start > 15000) return rej(new Error('timeout: ' + what + ' (' + got.titles.join(' | ') + ')'));
          setTimeout(poll, 200);
        }, rej);
      })();
    });
  };
  var shows = function (got, title) { return got.titles.indexOf(title) >= 0; };
  var START = 'Can’t read where sessions start';
  var JEV = 'Jev can’t suggest for todos';
  say('background notices check: waiting');
  once(function (got) { return shows(got, START) && shows(got, JEV); }, 'both notices')
    .then(function (got) {
      check('a failed read of where sessions start is a notice', true);
      check('a failed suggestion from Jev is a notice', true);
      var text = got.texts.join(' | ');
      check('each says what failed and why', /the last answer read stands: the engine is busy/.test(text) && /Jev couldn’t suggest .*: the engine is busy/.test(text), text);
      failing.suggest_todo = false;
      failing.start_options = false;
      // Another todo asks Jev again: a press selects its row.
      all('.ca-todorow-title')[1].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return once(function (now) { return !shows(now, JEV); }, 'Jev’s notice to go');
    })
    .then(function (got) {
      check('Jev’s goes once it answers', true);
      check('the other stays until its read works', shows(got, START), got.titles.join(' | '));
      var prs = all('.ca-nav').filter(function (b) { return /PRs/.test(b.textContent); })[0];
      prs.click();
      return until(function () { return q('.ca-prrow'); }, 10000, 'a PR row');
    })
    .then(function (row) {
      // Its menu has Review with, which reads where sessions start.
      return menuOf(row).then(function () {
        return until(function () { return !q('.ca-notice-open'); }, 10000, 'the start options notice to go');
      });
    })
    .then(function () { check('and it goes once a PR’s menu reads them', true); })
    .then(done)
    .catch(stopped);
})();
