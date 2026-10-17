// Checks that the quick panel leaves a snoozed session out of what waits on you until its time,
// as the floating bar and the waiting window do (quick.ts `groupOf`), and says it's snoozed
// wherever it still lists it: with nothing typed it's in Last 24 hours, not Needs you, and a
// search finds it after the ones that wait on you, each time with the snooze chip
// (LaneRow.tsx `SnoozeChip`) that the main window's rows have.
//   scripts/preview.sh quick '' scripts/preview/quick-snoozed-check.js
(function () {
  // This runs before the app's script: two of the board's sessions get titles nothing else has
  // and both need you, one of them snoozed for an hour, and the rest are archived so they can't
  // fill the lists ahead of them. The board has no todos or PRs and the text search finds
  // nothing, so a search for the word lists those two.
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.quick.archived');
  var WORD = 'pangolin';
  var AWAKE = 'Feed the ' + WORD;
  var NAPPING = 'Nap the ' + WORD;
  var named = 0;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.todos = [];
      board.prs = [];
      var now = Date.now();
      var n = 0;
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (l.hidden || l.device || s.visitor || s.helper || s.headless || n >= 2) {
            s.archived = true;
            return;
          }
          s.title = n === 0 ? AWAKE : NAPPING;
          s.status = 'needs';
          s.archived = false;
          s.startedAt = now - 60 * 60 * 1000;
          s.statusSince = now - (n === 0 ? 90 : 120) * 1000;
          if (n === 1) s.snoozedUntil = now + 60 * 60 * 1000;
          else delete s.snoozedUntil;
          n++;
        });
      });
      named = n;
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-row')); };
  var row = function (text) { return rows().filter(function (r) { return r.textContent.indexOf(text) >= 0; })[0]; };
  var groupOf = function (r) { var g = r && r.closest('.ca-quick-group'); return g ? g.getAttribute('aria-label') : null; };
  var chip = function (r) { return r && r.querySelector('.ca-snoozechip'); };

  say('quick snoozed check: waiting for the list');
  until(function () { return named === 2 && !!input() && !!row(AWAKE) && !!row(NAPPING); }, 20000, 'both sessions listed')
    .then(function () { return wait(300); })
    .then(function () {
      check('the one awake waits under Needs you', groupOf(row(AWAKE)) === 'Needs you', String(groupOf(row(AWAKE))));
      check('the snoozed one is out of Needs you, in Last 24 hours', groupOf(row(NAPPING)) === 'Last 24 hours', String(groupOf(row(NAPPING))));
      check('it has the snooze chip, saying how long until it’s back', !!chip(row(NAPPING)) && /1h|59m|60m/.test(chip(row(NAPPING)).textContent), row(NAPPING).innerHTML);
      check('the chip’s tooltip says until when and what it’s out of', !!chip(row(NAPPING)) && /^Snoozed until .*out of what waits on you/.test(chip(row(NAPPING)).getAttribute('title') || ''), chip(row(NAPPING)) && chip(row(NAPPING)).getAttribute('title'));
      check('the one awake has none', !chip(row(AWAKE)));
      type(input(), WORD);
      return until(function () { return groupOf(row(AWAKE)) === 'Sessions' && groupOf(row(NAPPING)) === 'Sessions'; }, 5000, 'both found by the search');
    })
    .then(function () {
      check('a search finds the snoozed one after the one that waits on you', rows().indexOf(row(NAPPING)) > rows().indexOf(row(AWAKE)));
      check('with the snooze chip there too', !!chip(row(NAPPING)) && !chip(row(AWAKE)));
    })
    .catch(function (e) { check('ran', false, String(e) + ' rows: ' + rows().map(function (r) { return groupOf(r) + ': ' + r.textContent.slice(0, 60); }).join(' / ')); })
    .then(done);
})();
