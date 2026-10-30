// A PR's primary session on another machine (boardLists.ts `prRowOf`, from the devices view's
// `prSessions`, peerfeed.rs `pr_sessions_for_windows`): where this Mac has no primary of the PR,
// its row names that machine's first and its panel tags it, saying it's that machine's; once this
// Mac has one, its own is the one tagged. The preview's devices stub has those Macs say their
// copies of the board's sessions are the primaries the board has; the check takes the PR's own
// primary out of every board the page gets, as a Mac with none of its sessions has it, then puts
// it back.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh prs '' scripts/preview/pr-other-primary-check.js
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
  // The panel's chip tagged primary, once the panel shows the PR.
  var tagged = function (pr) {
    var head = q('.ca-prinsp-head');
    if (!head || head.textContent.indexOf(pr.title) < 0) return null;
    var tag = q('.ca-places .ca-places-primary', head);
    return tag && tag.closest('.ca-chip');
  };

  say('Other machine’s PR primary check: waiting for the list');
  Promise.all([window.__TAURI_INTERNALS__.invoke('get_board'), window.__TAURI_INTERNALS__.invoke('devices')])
    .then(function (got) {
      var board = got[0];
      var view = got[1];
      if (!view.enabled) return stopped('run it with PREVIEW_DEVICES=scripts/preview/devices-sample.json');
      var theirs = function (p) { return (view.prSessions[p.key.toLowerCase()] || []).filter(function (x) { return x.primary; })[0]; };
      var pr = (board.prs || []).filter(theirs)[0];
      if (!pr) return stopped('no PR here has its primary session in the worktrees the sample puts on another Mac');
      var there = theirs(pr);
      var lane = view.lanes.filter(function (l) { return l.id === there.lane; })[0];
      var session = lane.sessions.filter(function (s) { return s.key === there.key; })[0];
      var machine = lane.device.name;
      // The board as a Mac with no primary of the PR has it, whenever the page reads it again.
      var strip = true;
      var without = function (b) {
        if (!strip) return b;
        var copy = JSON.parse(JSON.stringify(b));
        copy.prs.forEach(function (p) {
          if (p.key !== pr.key) return;
          delete p.primary;
          delete p.primaryBy;
        });
        return copy;
      };
      var invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd) {
        var got = invoke.apply(this, arguments);
        return cmd === 'get_board' ? got.then(without) : got;
      };
      window.__emit('board', without(board));
      return until(function () { return rowOf(pr); }, 20000, 'the row of #' + pr.number)
        .then(function (row) {
          var first = q('.ca-prrow-agents > .ca-chip:not(.ca-chip--more)', row);
          check('its row names ' + machine + '’s primary session', !!first && first.textContent.indexOf(session.title) >= 0, first ? first.textContent : 'no chip');
          press(row);
          return until(function () { return tagged(pr); }, 5000, 'a primary chip in the panel of #' + pr.number);
        })
        .then(function (chip) {
          var title = chip.getAttribute('title') || '';
          check('its panel tags ' + machine + '’s primary session', chip.textContent.indexOf(session.title) >= 0, chip.textContent);
          check('and says it’s that machine’s, to nudge and to pick', title.indexOf('The PR’s primary session on ' + machine + ': when that machine nudges the PR') >= 0 && title.indexOf('menu there picks another') >= 0, title.replace(/\n/g, ' | '));
          if (!board.prs.filter(function (p) { return p.key === pr.key; })[0].primary) return note('the board has no primary of #' + pr.number + ' of its own to put back');
          // This Mac's own primary back: it's the one tagged, as this Mac's.
          strip = false;
          window.__emit('board', board);
          var ours = function () {
            var c = tagged(pr);
            return c && (c.getAttribute('title') || '').indexOf('primary session on ' + machine) < 0 && c;
          };
          return until(ours, 5000, 'this Mac’s primary tagged again').then(function (own) {
            var words = own.getAttribute('title') || '';
            check('with this Mac’s primary back, its own is the one tagged', words.indexOf('Primary: The PR’s primary session: when the PR is nudged') >= 0, words.replace(/\n/g, ' | '));
          });
        });
    })
    .then(done)
    .catch(stopped);
})();
