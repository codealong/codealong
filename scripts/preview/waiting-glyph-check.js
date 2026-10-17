// Checks that a session waiting on a command its turn left running in the background (a CI
// watch) is drawn with a clock that stands still, not the working spinner, and says what it waits
// on; that a session whose turn is running keeps its spinner; and that each one's worktree goes by
// the same rule (`waitsInBackground`, `laneWaits` in lanes.ts). With the bar's feed, a waiting
// item's pill draws the clock too.
//   scripts/preview.sh sessions '' scripts/preview/waiting-glyph-check.js
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/waiting-glyph-check.js PREVIEW_NAME=waiting-glyph-dark scripts/preview.sh sessions
(function () {
  // This runs before the app's script: the first two worktrees on the board each get one working
  // session, the first waiting on `gh pr checks --watch`, the second busy with a turn; every
  // other session in them is idle, and nothing is pinned.
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var waits = 'gh pr checks 1151 --repo acme/web --watch --fail-fast --interval 60';
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'bar_feed') {
      return p.then(function (feed) {
        if (feed && feed.items && feed.items[0]) Object.assign(feed.items[0], { status: 'working', waiting: true, title: 'Waiting session' });
        return feed;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.pinned = [];
      b.lanes.forEach(function (l) {
        var eligible = l.sessions.filter(function (s) { return !s.visitor && !s.archived; });
        var take = !l.hidden && !l.device && eligible.length > 0 && picked.length < 2;
        l.sessions.forEach(function (s) {
          delete s.background;
          if (take || s.status === 'working') s.status = 'idle';
        });
        if (!take) return;
        var s = eligible[0];
        var waiting = picked.length === 0;
        var doing = waiting ? 'Waiting on `' + waits + '`' : 'reading `WorkflowService.ts`';
        Object.assign(s, { status: 'working', statusSince: now - 9 * 60e3, lastActivity: now - 60e3, snoozedUntil: undefined, excerpt: doing, detail: '{for:' + (now - 9 * 60e3) + '} · ' + doing });
        s.title = waiting ? 'Waiting session' : 'Busy session';
        if (waiting) s.background = waits;
        Object.assign(l, { glyph: 'working', bay: 'working' });
        picked.push({ key: s.key, lane: l.id });
      });
      return b;
    });
  };

  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var glyph = function (el) { var g = el && el.querySelector('.ca-seg-glyph svg, svg.ca-glyph'); return g ? g.getAttribute('class') : null; };

  say('waiting glyph check: waiting for the list');
  until(function () { return picked.length === 2 && row(0) && row(1); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      check('the waiting session draws a clock', /ca-glyph--waiting/.test(glyph(row(0)) || ''), glyph(row(0)));
      check('which stands still', getComputedStyle(row(0).querySelector('svg.ca-glyph')).animationName === 'none');
      check('and says what it waits on', /Waiting on gh pr checks 1151/.test(row(0).textContent), row(0).textContent.slice(0, 160));
      check('a session whose turn runs keeps its spinner', /ca-glyph--working/.test(glyph(row(1)) || ''), glyph(row(1)));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
