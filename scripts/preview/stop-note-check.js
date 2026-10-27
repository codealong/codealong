// Checks where a turn was stopped in a session's conversation (SessionThread.tsx, the `stop`
// note transcript.rs `stopped` makes): under the call it cut short, the note says who stopped it
// when Codealong did (stops.rs `attribute`), with a stop sign, set apart in color from the other
// notes, which keep their look; one Codealong didn't make just says the turn was stopped.
//   scripts/preview.sh sessions '' scripts/preview/stop-note-check.js
// As PREVIEW_INIT, after a line setting `window.__stop_screenshot = true`, it leaves the notes in
// view for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  // The first session's conversation goes on with a call cut short and the notes after it.
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key && (args || {}).before == null) {
      return p.then(function (page) {
        var at = Date.now() - 30e3;
        var item = function (kind, text, n, extra) { return Object.assign({ at: at + n, kind: kind, text: text, failed: false, cursor: 10 + n, n: 0 }, extra || {}); };
        page.items = page.items.concat([
          item('you', 'Ship it once the checks pass.', 0),
          item('tool', 'Bash · `npm run release`', 1, { id: 'cut', input: 'npm run release' }),
          item('result', '', 2, { id: 'cut', failed: true, output: 'The user doesn’t want to proceed with this tool use.' }),
          item('note', 'Stopped from Codealong (a window on this Mac)', 3, { stop: true }),
          item('you', 'Ok, go on.', 4),
          item('note', 'The turn was stopped', 5, { stop: true }),
          item('note', 'The conversation was compacted here', 6),
        ]);
        page.after = 20;
        return page;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var notes = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-note')); };
  var stops = function () { return notes().filter(function (n) { return n.classList.contains('is-stop'); }); };

  say('stop note check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return stops().length === 2; }, 5000);
    })
    .then(function () {
      var ours = stops()[0];
      var theirs = stops()[1];
      var plain = notes().filter(function (n) { return !n.classList.contains('is-stop'); }).pop();
      check('says Codealong stopped it, and who asked', ours.textContent === 'Stopped from Codealong (a window on this Mac)', ours.textContent);
      check('one Codealong didn’t make says only that it was stopped', theirs.textContent === 'The turn was stopped', theirs.textContent);
      check('each has a stop sign', !!ours.querySelector('svg') && !!theirs.querySelector('svg'));
      check('the other notes keep their look', !!plain && plain.textContent === 'The conversation was compacted here' && !plain.querySelector('svg'), plain && plain.outerHTML);
      var c = function (el) { var s = getComputedStyle(el); return s.color + ' on ' + s.backgroundColor; };
      check('set apart in color from the other notes', c(ours) !== c(plain), c(ours) + ' / ' + c(plain));
      var before = ours.previousElementSibling;
      check('under the call it cut short', !!before && !!before.closest('.ca-sessinsp') && /npm run release/.test(before.textContent), before && before.className);
      if (window.__stop_screenshot) {
        var thread = document.querySelector('.ca-sessinsp .ca-thread');
        thread.scrollTop = Math.max(0, ours.offsetTop - 160);
        return 'stay';
      }
    })
    .then(function (stay) {
      if (stay) return;
      done();
    })
    .catch(stopped);
})();
