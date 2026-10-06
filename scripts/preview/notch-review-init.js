// Opens the bar docked in the notch on a PR waiting on your review, for a screenshot of it
// (preview.sh's notch view): the board's first PR is said to ask for your review, whatever it
// is, and the notch opens on it.
//   PREVIEW_SIZE=1512,720 PREVIEW_INIT=scripts/preview/notch-review-init.js scripts/preview.sh notch
//   PREVIEW_SCHEME=dark PREVIEW_SIZE=1512,720 PREVIEW_INIT=scripts/preview/notch-review-init.js scripts/preview.sh notch
(function () {
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = json('./board.json');
  var pr = (board.prs || []).filter(function (p) { return p.state !== 'merged' && p.state !== 'closed'; })[0] || (board.prs || [])[0];
  if (!pr) return;
  var asked = { reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600000, snoozedUntil: null, state: pr.state === 'merged' || pr.state === 'closed' ? 'open' : pr.state };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd === 'get_board') {
      return p.then(function (b) {
        b.prs = b.prs.map(function (x) { return x.key === pr.key ? Object.assign({}, x, asked) : x; });
        return b;
      });
    }
    if (cmd === 'bar_feed') {
      return p.then(function (f) {
        f.reviews = [{ key: pr.key, number: pr.number, title: pr.title, repo: pr.repo, repoSlug: pr.repoSlug, url: pr.url, project: pr.project, requestedBy: 'ana', since: asked.requestedAt, updatedAt: pr.updatedAt }];
        return f;
      });
    }
    return p;
  };
  var opened = false;
  var t = setInterval(function () {
    var shape = document.querySelector('.ca-notch-shape');
    if (!shape || !document.querySelector('.ca-notch-count')) return;
    if (!opened) {
      opened = true;
      shape.click();
      return;
    }
    var pill = document.querySelector('.ca-notch-body .ca-hud-pill.is-reviews');
    if (!pill) return;
    clearInterval(t);
    pill.click();
  }, 60);
})();
