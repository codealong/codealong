// The sessions on your other Macs that worked on a PR, on its row and in its panel beside this
// Mac's (boardLists.ts `prRowOf`, from the devices view's `prSessions`, peerfeed.rs
// `pr_sessions_for_windows`): each under its worktree there, whose chip wears that Mac's badge. The
// preview's devices stub has those Macs say their copies of the board's worktrees worked on the
// PRs the board's sessions in them did.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh prs '' scripts/preview/pr-other-machines-check.js
(function () {
  localStorage.removeItem('codealong.prDetailsOpen');
  localStorage.setItem('codealong.prRows', 'two');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var rowOf = function (pr) {
    return all('.ca-prrow').filter(function (r) { var n = q('.ca-prrow-num', r); return n && n.textContent === '#' + pr.number && q('.ca-lane-headline', r).textContent === pr.title; })[0];
  };

  say('PR sessions on other machines check: waiting for the list');
  Promise.all([window.__TAURI_INTERNALS__.invoke('get_board'), window.__TAURI_INTERNALS__.invoke('devices')])
    .then(function (got) {
      var board = got[0];
      var view = got[1];
      if (!view.enabled) return stopped('run it with PREVIEW_DEVICES=scripts/preview/devices-sample.json');
      var pr = (board.prs || []).filter(function (p) { return (view.prSessions[p.key.toLowerCase()] || []).length > 0; })[0];
      if (!pr) return stopped('no PR here has a session in the worktrees the sample puts on another Mac');
      var there = view.prSessions[pr.key.toLowerCase()];
      var lane = view.lanes.filter(function (l) { return l.id === there[0].lane; })[0];
      return until(function () { return rowOf(pr); }, 20000, 'the row of #' + pr.number)
        .then(function (row) {
          var more = q('.ca-prrow-agents .ca-chip--more', row);
          var named = (more && more.getAttribute('title')) || '';
          var counted = all('.ca-prrow-agents > .ca-chip:not(.ca-chip--more)', row).length + (more ? Number((more.textContent.match(/\d+/) || ['0'])[0]) : 0);
          check('its row counts the other machine’s sessions with this one’s', counted === pr.sessions.length + there.length, counted + ' of ' + (pr.sessions.length + there.length) + (named ? ' (' + named.replace(/\n/g, ' | ') + ')' : ''));
          press(row);
          // The panel of the PR clicked, not the one it showed before.
          return until(function () {
            var head = q('.ca-prinsp-head');
            return head && head.textContent.indexOf(pr.title) >= 0 && q('.ca-places', head);
          }, 5000, 'the panel of #' + pr.number + ' with its chips');
        })
        .then(function (places) {
          var groups = all('.ca-places-group', places).filter(function (g) { var d = q('.ca-wtchip-device', g); return d && d.textContent.indexOf(lane.device.name) >= 0; });
          check('its panel lists the worktree there, with ' + lane.device.name + '’s badge', groups.length > 0, all('.ca-places-group .ca-wtchip', places).map(function (c) { return c.textContent; }).join(' | '));
          var chips = groups.reduce(function (n, g) { return n + all('.ca-chip:not(.ca-wtchip)', g).length; }, 0);
          check('and under it the sessions that machine says worked on it', chips === there.length, chips + ' chips, ' + there.length + ' sessions');
          var heading = q('.ca-prinsp .ca-places');
          check('the chips are named for what they are, wherever they are', heading && heading.getAttribute('aria-label') === 'Worktrees and sessions', heading ? heading.getAttribute('aria-label') : 'none');
        });
    })
    .then(done)
    .catch(stopped);
})();
