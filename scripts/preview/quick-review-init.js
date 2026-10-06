// Opens the quick panel on a PR waiting on your review, for a screenshot of its card there
// (preview.sh's quick view): the board's first open PR is said to ask for your review.
//   PREVIEW_INIT=scripts/preview/quick-review-init.js scripts/preview.sh quick
//   PREVIEW_SCHEME=dark PREVIEW_NAME=quick-review-dark PREVIEW_INIT=scripts/preview/quick-review-init.js scripts/preview.sh quick
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var key = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var done = false;
      board.prs = (board.prs || []).map(function (x) {
        if (done) return Object.assign({}, x, { reviewRequested: false });
        done = true;
        key = x.key;
        return Object.assign({}, x, { state: x.state === 'merged' || x.state === 'closed' ? 'open' : x.state, reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null });
      });
      return board;
    });
  };
  var t = setInterval(function () {
    var rows = Array.prototype.slice.call(document.querySelectorAll('.ca-quick-row.is-pr'));
    var input = document.querySelector('.ca-quick-input textarea');
    if (!key || !input || rows.length === 0) return;
    clearInterval(t);
    rows[0].click();
  }, 60);
})();
