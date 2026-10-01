// Checks the notices in the accounts bar (ui/Notices.tsx): several are one chip that says how
// many, its card lists each, a notice dismissed there leaves the others, config.toml's opens
// Settings on the file, and the chip's own button dismisses what's left. Dismissals are kept.
//   PREVIEW_NOTICES=scripts/preview/notices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/notices-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 20); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var q = function (sel) { return document.querySelector(sel); };
  var all = function (sel) { return Array.prototype.slice.call(document.querySelectorAll(sel)); };
  var label = function () { var c = q('.ca-notice-open'); return c ? c.textContent : null; };
  var kept = function () { return JSON.parse(window.localStorage.getItem('codealong.dismissedNotices') || '[]'); };
  var button = function (text) { return all('.ca-notice-card button').filter(function (b) { return b.textContent === text; })[0]; };
  window.localStorage.removeItem('codealong.dismissedNotices');

  until(function () { return q('.ca-notice-open'); }, 20000)
    .then(function () {
      check('several are one chip', label() === '2 problems', label());
      check('nothing in the sidebar', !q('.ca-side-warnings'));
      q('.ca-notice-open').click();
      return until(function () { return q('.ca-notice-card'); }, 3000);
    })
    .then(function () {
      var titles = all('.ca-notice-title').map(function (t) { return t.textContent; });
      check('the card lists each', titles.join(' | ') === 'GitHub rate limit reached | config.toml has a problem', titles.join(' | '));
      check('and says until when', /^Until \d+:\d\d [AP]M, in 1[12]m$/.test((q('.ca-notice-until') || {}).textContent || ''), (q('.ca-notice-until') || {}).textContent);
      button('Dismiss').click();
      return until(function () { return all('.ca-notice-item').length === 1; }, 3000);
    })
    .then(function () {
      check('one dismissed, the other stays', label() === 'config.toml has a problem' && !!q('.ca-notice-card'), label());
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
      check('the chip dismisses what is left', kept().length === 2, JSON.stringify(kept()));
    })
    .catch(function (e) { out.push('FAIL ' + e.message + ' at: ' + (label() || 'no chip')); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
