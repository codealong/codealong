// Checks headless runs in Sessions (boardLists.ts `placeHelpers`, `runCounts`), on the board
// runs-init.js makes: the runs a session's command started aren't listed, a row a call, but
// counted on its row (LaneRow.tsx `RunsChip`), with the working glyph while any is at work and a
// tooltip saying how many and what they run on; a double-click lists them all under it, set in,
// and remembers that apart from its helpers, and another takes them away; a run no session on the
// board started is listed as a session of its own, with a chip saying it's a headless run; and
// none of them counts as an agent at work, in the Working filter or on this Mac's line.
//   PREVIEW_INIT=scripts/preview/runs-init.js PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/runs-check.js
(function () {
  // By id: another device's copy of the worktree (PREVIEW_DEVICES) has rows with the same titles.
  var rowOf = function (title) {
    var r = window.__runs;
    var s = [r.boss, r.lone].concat(r.runs).filter(function (x) { return x.title === title; })[0];
    return document.getElementById('row-' + r.lane + '::' + s.key);
  };
  var dblclick = function (el) { el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true })); };
  (async function () {
    try {
      var r = await until(function () { return window.__runs && all('.ca-sessionlist .ca-lane.is-agent').length > 0 && window.__runs; }, 30000, 'the board with headless runs');
      var boss = await until(function () { return rowOf(r.boss.title); }, 10000, 'the session whose command ran them');
      check('its runs aren’t listed', r.runs.every(function (x) { return !rowOf(x.title); }), r.runs.filter(function (x) { return rowOf(x.title); }).map(function (x) { return x.title; }).join(' '));
      var chip = q('.ca-runschip', boss);
      check('its row counts them, done ones too', !!chip && /4 runs/.test(chip.textContent), chip && chip.textContent);
      check('with the working glyph while one is at work', !!chip && !!q('.ca-glyph--working', chip));
      check('its tooltip says how many are at work and what they run on', !!chip && /^4 headless runs its commands started, 3 at work/.test(chip.title) && /claude-opus-5-5/.test(chip.title), chip && chip.title);
      check('not on its helpers’ chip', !q('.ca-helperschip', boss) || !/4/.test(q('.ca-helperschip', boss).textContent));

      // A double-click lists them all under it, and another takes them away.
      dblclick(chip);
      var listed = await until(function () { var rows = r.runs.map(function (x) { return rowOf(x.title); }); return rows.every(Boolean) && rows; }, 5000, 'its runs listed after a double-click');
      check('a double-click lists them all under it, set in', listed.every(function (row) { return row.classList.contains('is-nested'); }) && q('.ca-runschip', rowOf(r.boss.title)).classList.contains('is-shown'));
      var stored = localStorage.getItem('codealong.helpersShown') || '';
      check('and remembers it apart from its helpers', stored.indexOf('runs:' + r.boss.key) >= 0 && JSON.parse(stored).indexOf(r.boss.key) < 0, stored);
      check('a listed run has no chip of its own', listed.every(function (row) { return !q('.ca-helperchip', row); }));
      dblclick(q('.ca-runschip', rowOf(r.boss.title)));
      await until(function () { return r.runs.every(function (x) { return !rowOf(x.title); }); }, 5000, 'its runs gone again');
      check('another takes them away', !q('.ca-runschip', rowOf(r.boss.title)).classList.contains('is-shown'));

      // One no session here started is a session of its own, saying what it is.
      var lone = rowOf(r.lone.title);
      var tag = lone && q('.ca-helperchip', lone);
      check('a run no session started is listed', !!lone && !lone.classList.contains('is-nested'));
      check('with a chip saying it’s a headless run', !!tag && /Headless/.test(tag.textContent) && /claude -p/.test(tag.title), tag && tag.textContent);

      // None of them is an agent of yours at work: this Mac's line counts the rest.
      var board = await window.__TAURI_INTERNALS__.invoke('get_board');
      var working = 0;
      board.lanes.forEach(function (l) {
        if (l.device) return;
        l.sessions.forEach(function (s) {
          if (!s.visitor && !s.archived && !s.helper && !s.headless && s.status === 'working') working++;
        });
      });
      var here = all('.ca-side-dev-sub').filter(function (e) { return /^This Mac · /.test(e.textContent); })[0];
      if (!here) note('no devices in the sidebar, so this Mac’s line wasn’t checked');
      else check('this Mac’s line doesn’t count them as at work', here.textContent === 'This Mac · ' + working + ' working', here.textContent + ' / ' + working);
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
