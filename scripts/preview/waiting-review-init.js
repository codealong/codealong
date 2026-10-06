// Opens the waiting window on a PR waiting on your review, for a screenshot of its card there
// (preview.sh's waiting view): the board's first open PR is said to ask for your review, and no
// session waits on you, so the review is the first that waits.
//   PREVIEW_INIT=scripts/preview/waiting-review-init.js PREVIEW_NAME=waiting-review scripts/preview.sh waiting
//   PREVIEW_SCHEME=dark PREVIEW_NAME=waiting-review PREVIEW_INIT=scripts/preview/waiting-review-init.js scripts/preview.sh waiting
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        });
      });
      var done = false;
      board.prs = (board.prs || []).map(function (x) {
        if (done) return Object.assign({}, x, { reviewRequested: false });
        done = true;
        return Object.assign({}, x, { state: x.state === 'merged' || x.state === 'closed' ? 'open' : x.state, reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null });
      });
      return board;
    });
  };
})();
