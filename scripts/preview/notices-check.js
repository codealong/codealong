// Checks the notices in the accounts bar (ui/Notices.tsx): several are one chip that says how
// many, its card lists each, a review that didn't start is started again from its Try again, a
// notice dismissed there leaves the others, config.toml's opens Settings on the file, and the
// chip's own button dismisses what's left. Dismissals are kept.
//   PREVIEW_NOTICES=scripts/preview/notices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/notices-check.js
(function () {
  var label = function () { var c = q('.ca-notice-open'); return c ? c.textContent : null; };
  var kept = function () { return JSON.parse(window.localStorage.getItem('codealong.dismissedNotices') || '[]'); };
  var button = function (text) { return all('.ca-notice-card button').filter(function (b) { return b.textContent === text; })[0]; };
  var openCard = function () {
    q('.ca-notice-open').click();
    return until(function () { return q('.ca-notice-card'); }, 3000);
  };
  window.localStorage.removeItem('codealong.dismissedNotices');

  until(function () { return q('.ca-notice-open'); }, 20000)
    .then(function () {
      check('several are one chip', label() === '3 problems', label());
      check('nothing in the sidebar', !q('.ca-side-warnings'));
      return openCard();
    })
    .then(function () {
      var titles = all('.ca-notice-title').map(function (t) { return t.textContent; });
      check('the card lists each', titles.join(' | ') === "GitHub rate limit reached | config.toml has a problem | A pull request wasn't reviewed", titles.join(' | '));
      check('and says until when', /^Until \d+:\d\d [AP]M, in 1[12]m$/.test((q('.ca-notice-until') || {}).textContent || ''), (q('.ca-notice-until') || {}).textContent);
      var again = all('.ca-notice-item').filter(function (s) { return all('button', s).some(function (b) { return b.textContent === 'Try again'; }); });
      check('only the review that did not start offers Try again', again.length === 1 && /wasn't reviewed/.test(again[0].textContent), again.length);
      button('Try again').click();
      return until(function () { return calls('review_pr_now').length > 0; }, 3000);
    })
    .then(function () {
      var asked = sent('review_pr_now')[0];
      check('Try again starts its review on the reviewer Settings names', asked.key === 'acme/web#1238' && asked.account === null, JSON.stringify(asked));
      check('and closes the card', !q('.ca-notice-card'));
      return until(function () { return /Reviewing #1238/.test(document.body.textContent); }, 3000, 'the toast');
    })
    .then(function () {
      return openCard();
    })
    .then(function () {
      button('Dismiss').click();
      return until(function () { return all('.ca-notice-item').length === 2; }, 3000);
    })
    .then(function () {
      check('one dismissed, the others stay', label() === '2 problems' && !!q('.ca-notice-card'), label());
      check('the dismissal is kept', kept().length === 1 && kept()[0].indexOf('github') === 0, JSON.stringify(kept()));
      button('Open config.toml').click();
      return until(function () { return q('.ca-dialog'); }, 3000);
    })
    .then(function () {
      check('its file opens in Settings', !q('.ca-notice-card') && /config\.toml/.test(q('.ca-dialog').textContent));
      q('.ca-notice-x').click();
      return until(function () { return !q('.ca-notice'); }, 3000);
    })
    .then(function () {
      check('the chip dismisses what is left', kept().length === 3, JSON.stringify(kept()));
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' at: ' + (label() || 'no chip')); });
})();
