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
  // What the notices' card lists, by `part` (title or text); the card opens and closes for it.
  var read = function (part) {
    var chip = q('.ca-notice-open');
    if (!chip) return [];
    if (!q('.ca-notice-card')) chip.click();
    var list = all('.ca-notice-card .ca-notice-' + part).map(function (el) { return el.textContent; });
    chip.click();
    return list;
  };
  var shows = function (title) { return read('title').indexOf(title) >= 0; };
  var START = 'Can’t read where sessions start';
  var JEV = 'Jev can’t suggest for todos';
  say('background notices check: waiting');
  until(function () { return shows(START) && shows(JEV); }, 20000, 'both notices')
    .then(function () {
      check('a failed read of where sessions start is a notice', true);
      check('a failed suggestion from Jev is a notice', true);
      var text = read('text').join(' | ');
      check('each says what failed and why', /the last answer read stands: the engine is busy/.test(text) && /Jev couldn’t suggest .*: the engine is busy/.test(text), text);
      failing.suggest_todo = false;
      failing.start_options = false;
      // Another todo asks Jev again.
      all('.ca-todorow-title')[1].click();
      return until(function () { return !shows(JEV); }, 10000, 'Jev’s notice to go');
    })
    .then(function () {
      check('Jev’s goes once it answers', true);
      check('the other stays until its read works', shows(START), read('title').join(' | '));
      var prs = all('.ca-nav').filter(function (b) { return /PRs/.test(b.textContent); })[0];
      prs.click();
      return until(function () { return q('.ca-bay-lanes > *'); }, 10000, 'a PR row');
    })
    .then(function (row) {
      rightClick(row);
      return until(function () { return !q('.ca-notice-open'); }, 10000, 'the start options notice to go');
    })
    .then(function () { check('and it goes once a PR’s menu reads them', true); })
    .then(done)
    .catch(stopped);
})();
