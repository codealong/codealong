// Checks that a session shown with its conversation says which it is (SessionPanel.tsx
// `data-shows-session`), as Improve Codealong asks each window (improve.rs `SHOWN`, the same
// script) to picture the terminal tab the session runs in: in Sessions, the panel of the row
// picked; in the peek, its card.
//   scripts/preview.sh sessions '' scripts/preview/shows-session-check.js
//   scripts/preview.sh peek '' scripts/preview/shows-session-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.removeItem('codealong.sessionPanelMax');
  // improve.rs `SHOWN`, as the app runs it in each window's page.
  var shown = function () { return JSON.parse(JSON.stringify(Array.from(document.querySelectorAll('[data-shows-session]'), (e) => e.dataset.showsSession))); };
  // A session row's id is `row-<lane>::<key>`.
  var rows = function () { return all('[id^="row-"]').filter(function (r) { return r.id.indexOf('::') > 0; }); };

  (async function () {
    try {
      var board = await window.__TAURI_INTERNALS__.invoke('get_board');
      var keys = {};
      board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { keys[s.key] = s.title; }); });
      await until(function () { return rows().length > 0 || !!q('.ca-reply'); }, 20000, 'a session’s row, or the peek’s card');
      if (rows().length > 0) {
        var row = rows()[rows().length > 1 ? 1 : 0];
        var key = row.id.slice(row.id.indexOf('::') + 2);
        row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
        await until(function () { return JSON.stringify(shown()) === JSON.stringify([key]); }, 5000, 'the panel of the row picked');
        check('the panel names the session it shows, alone', JSON.stringify(shown()) === JSON.stringify([key]), JSON.stringify(shown()) + ' for ' + key);
        check('by its key on the board', keys[key] !== undefined, key);
      } else {
        var marked = shown();
        check('the card names the session it shows', marked.length === 1 && keys[marked[0]] !== undefined, JSON.stringify(marked));
      }
    } catch (e) {
      check('ran', false, String(e));
    }
    done();
  })();
})();
