// Checks ⌘R (reload.ts) on a PR waiting on your review, in the main window's PRs and on its card
// in the waiting window: with GitHub out of reach, the PR's details and its merge box say why;
// ⌘R is taken from the page, so View's Reload doesn't run it again, asks the engine to poll GitHub
// again, and for usage too in the board's window but not the waiting window, which shows none;
// it reads the details, the merge box and your review again, and with GitHub back what's read
// takes the errors' place. A second ⌘R with GitHub gone again keeps what was read up beside the
// error.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/reload-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/reload-check.js
(function () {
  var board = !window.__CODEALONG_WAITING__;
  var down = true;
  var FAILURE = 'GitHub: error connecting to api.github.com';
  var BODY = 'Read again after the reload.';
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail' || cmd === 'merge_box' || cmd === 'pr_draft') {
      if (down) {
        window.__calls.push([cmd, args, options]);
        return Promise.reject(new Error(FAILURE));
      }
      if (cmd === 'pr_detail') {
        window.__calls.push([cmd, args, options]);
        return Promise.resolve({ bodyHtml: '<p>' + BODY + '</p>', mergeable: null, commits: 2, reviews: [], requested: [], comments: [], files: [], moreFiles: false });
      }
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        });
      });
      // The first PR asks for your review, and no other does.
      b.prs = (b.prs || []).map(function (x) {
        if (pr && x.key !== pr.key) return Object.assign({}, x, { reviewRequested: false });
        pr = Object.assign({}, x, { state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null });
        return pr;
      });
      return b;
    });
  };

  var errors = function () { return all('.ca-form-error').map(function (e) { return e.textContent; }).filter(function (t) { return t.indexOf(FAILURE) >= 0; }); };
  var detailFailed = function () { return errors().indexOf(FAILURE) >= 0; };
  var mergeFailed = function () { return errors().some(function (t) { return t.indexOf('Couldn’t read how it merges') === 0; }); };
  var shown = function () { return document.body.textContent.indexOf(BODY) >= 0; };
  var counts = function () { return { refresh: calls('refresh').length, usage: calls('refresh_usage').length, detail: calls('pr_detail').length, merge: calls('merge_box').length, draft: calls('pr_draft').length }; };
  var reload = function () {
    var e = new KeyboardEvent('keydown', { key: 'r', code: 'KeyR', metaKey: true, bubbles: true, cancelable: true });
    document.body.dispatchEvent(e);
    return e.defaultPrevented;
  };
  var before;

  say('reload check: waiting for the PR to fail to read');
  until(function () { return !!pr && detailFailed() && mergeFailed(); }, 20000, 'the PR’s errors')
    .then(function () {
      check('with GitHub out of reach the details and the merge box say why', detailFailed() && mergeFailed(), errors().join(' | '));
      before = counts();
      down = false;
      check('⌘R is taken from the page', reload());
      var saying = function () { return document.body.textContent.indexOf('Reloading…') >= 0; };
      if (board) return until(saying, 2000, 'the toast').then(function () { check('the window says it’s reloading', saying()); });
    })
    .then(function () { return until(function () { return shown() && errors().length === 0; }, 5000, 'what was read again'); })
    .then(function () {
      var after = counts();
      check('it asks the engine to poll GitHub again', after.refresh === before.refresh + 1, before.refresh + ' → ' + after.refresh);
      if (board) check('and to read usage again, in the board’s window', after.usage === before.usage + 1, before.usage + ' → ' + after.usage);
      else check('and not usage, in the waiting window', after.usage === before.usage, before.usage + ' → ' + after.usage);
      check('the details are read again', after.detail > before.detail, before.detail + ' → ' + after.detail);
      check('and the merge box', after.merge > before.merge, before.merge + ' → ' + after.merge);
      if (before.draft > 0) check('and your review on GitHub', after.draft > before.draft, before.draft + ' → ' + after.draft);
      else note('nothing read your review on GitHub before the reload');
      check('what was read takes the errors’ place', shown() && errors().length === 0, errors().join(' | '));
      before = after;
      down = true;
      reload();
      return until(function () { return calls('pr_detail').length > before.detail && detailFailed(); }, 5000, 'the second read failing');
    })
    .then(function () {
      check('read again and failing, what was read stays up beside the error', shown() && detailFailed(), errors().join(' | '));
      done();
    })
    .catch(stopped);
})();
