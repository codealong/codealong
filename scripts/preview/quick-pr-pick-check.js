// Checks what picking a found PR in the quick panel does (Quick.tsx): ⏎, a click and its ⌘
// number show it in Codealong and put the panel away with its search cleared, without opening it
// on GitHub; ⌥⏎ opens it on GitHub, without showing it in Codealong.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-pr-pick-check.js
// The board needs a PR.
(function () {
  // This runs before the app's script: the board's first PR gets a title nothing else has, so a
  // search finds it alone among the PRs, and it isn't waiting on your review (⏎ reviews one of
  // those). The preview's text search finds two sessions for any words, listed above it.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var WORD = 'kingfisher';
  var TITLE = 'Pick the ' + WORD;
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.prs = (board.prs || []).map(function (x, i) {
        if (picked ? x.key !== picked.key : i > 0) return x;
        picked = { key: x.key, url: x.url };
        return Object.assign({}, x, { title: TITLE, state: 'open', reviewRequested: false, mergedAt: null, closedAt: null });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var calls = sent;
  var last = function (cmd) { return calls(cmd).pop(); };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-row')); };
  var row = function () { return rows().filter(function (r) { return r.textContent.indexOf(TITLE) >= 0; })[0]; };
  // The search finds the PR, once the text search's sessions are in above it, and the pointer
  // over it makes it the row ⏎ does. (⌥⏎ leaves the words in the box, which a new open clears,
  // so they're cleared first.)
  var find = function () {
    type(input(), '');
    type(input(), WORD);
    return until(function () { return !!row() && !!document.querySelector('.ca-quick-row-why.is-said'); }, 5000, 'the PR and the text search')
      .then(function () { return wait(200); })
      .then(function () {
        pointAt(row());
        return until(function () { return row().classList.contains('is-active'); }, 2000, 'the PR selected');
      })
      .then(function () { return wait(100); });
  };
  var isPicked = function (shown) { return !!shown && !!shown.reveal && shown.reveal.kind === 'pr' && shown.reveal.key === picked.key; };
  var counts = function () { return { shows: calls('show_main').length, opens: calls('open_url').length, hides: calls('hide_quick').length }; };
  var before;
  // One pick: it should have shown the PR in Codealong, and not opened it on GitHub.
  var shows = function (name) {
    var now = counts();
    check(name + ' shows the PR in Codealong', now.shows === before.shows + 1 && isPicked(last('show_main')), JSON.stringify(last('show_main')));
    check(name + ' doesn’t open it on GitHub', now.opens === before.opens);
    check(name + ' puts the panel away', now.hides === before.hides + 1);
  };

  say('quick PR pick check: waiting for the list');
  until(function () { return !!picked && !!input() && rows().length > 0; }, 20000, 'the list and a PR')
    .then(function () { return wait(300); })
    .then(find)
    .then(function () {
      check('the row says what the keys do', /⏎ shows it in Codealong/.test(row().title) && /⌥⏎ opens it on GitHub/.test(row().title), row().title);
      before = counts();
      key(input(), { key: 'Enter' });
      return wait(300);
    })
    .then(function () {
      shows('⏎');
      check('and clears the search', input().value === '', input().value);
      return find();
    })
    .then(function () {
      before = counts();
      key(input(), { key: 'Enter', altKey: true });
      return wait(300);
    })
    .then(function () {
      var now = counts();
      var o = last('open_url');
      check('⌥⏎ opens it on GitHub', now.opens === before.opens + 1 && !!o && o.url === picked.url, JSON.stringify(o));
      check('⌥⏎ doesn’t show it in Codealong', now.shows === before.shows);
      check('⌥⏎ puts the panel away', now.hides === before.hides + 1);
      return find();
    })
    .then(function () {
      before = counts();
      pointAt(row());
      row().click();
      return wait(300);
    })
    .then(function () {
      shows('a click');
      return find();
    })
    .then(function () {
      var n = String(rows().indexOf(row()) + 1);
      before = counts();
      key(window, { key: n, code: 'Digit' + n, metaKey: true });
      return wait(300).then(function () { shows('⌘' + n); });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
