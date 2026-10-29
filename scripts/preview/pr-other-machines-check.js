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
      // The PR with sessions on the most machines, so one on both of the sample's says each apart.
      var spread = function (p) { return new Set((view.prSessions[p.key.toLowerCase()] || []).map(function (x) { return x.lane.split(':')[0]; })).size; };
      var pr = (board.prs || []).filter(function (p) { return spread(p) > 0; }).sort(function (a, b) { return spread(b) - spread(a); })[0];
      if (!pr) return stopped('no PR here has a session in the worktrees the sample puts on another Mac');
      var there = view.prSessions[pr.key.toLowerCase()];
      var laneOf = function (lanes, id) { return lanes.filter(function (l) { return l.id === id; })[0]; };
      // The machines it has sessions on, each with how many: the sample's two can both have some.
      var machines = {};
      there.forEach(function (x) { var name = laneOf(view.lanes, x.lane).device.name; machines[name] = (machines[name] || 0) + 1; });
      var names = Object.keys(machines);
      // This Mac's sessions on it with nowhere to open them, which have no arrow either.
      var shut = pr.sessions.filter(function (x) {
        var l = laneOf(board.lanes, x.lane);
        var found = l && l.sessions.filter(function (z) { return z.key === x.key; })[0];
        return found && !found.openTarget;
      }).length;
      // A worktree's chip or tree label says the machine it's on first (`On Mac mini: /path`).
      var on = function (el, name) { return !!el && (el.getAttribute('title') || '').indexOf('On ' + name + ': ') === 0; };
      var elsewhere = function (el) { return !!el && /^On [^:]+: /.test(el.getAttribute('title') || ''); };
      return until(function () { return rowOf(pr); }, 20000, 'the row of #' + pr.number)
        .then(function (row) {
          var more = q('.ca-prrow-agents .ca-chip--more', row);
          var named = (more && more.getAttribute('title')) || '';
          var counted = all('.ca-prrow-agents > .ca-chip:not(.ca-chip--more)', row).length + (more ? Number((more.textContent.match(/\d+/) || ['0'])[0]) : 0);
          check('its row counts the other machines’ sessions with this one’s', counted === pr.sessions.length + there.length, counted + ' of ' + (pr.sessions.length + there.length) + (named ? ' (' + named.replace(/\n/g, ' | ') + ')' : ''));
          press(row);
          // The panel of the PR clicked, not the one it showed before.
          return until(function () {
            var head = q('.ca-prinsp-head');
            return head && head.textContent.indexOf(pr.title) >= 0 && q('.ca-places', head);
          }, 5000, 'the panel of #' + pr.number + ' with its chips');
        })
        .then(function (places) {
          names.forEach(function (name) {
            var groups = all('.ca-places-group', places).filter(function (g) { return on(q('.ca-wtchip', g), name) && !!q('.ca-wtchip-device', g); });
            check('its panel lists the worktrees on ' + name + ', with its badge', groups.length > 0, all('.ca-places-group .ca-wtchip', places).map(function (c) { return c.getAttribute('title'); }).join(' | '));
            var chips = groups.reduce(function (n, g) { return n + all('.ca-chip:not(.ca-wtchip)', g).length; }, 0);
            check('and under them the sessions ' + name + ' says worked on it', chips === machines[name], chips + ' chips, ' + machines[name] + ' sessions');
          });
          var heading = q('.ca-prinsp .ca-places');
          check('the chips are named for what they are, wherever they are', heading && heading.getAttribute('aria-label') === 'Worktrees and sessions', heading ? heading.getAttribute('aria-label') : 'none');
          press(q('.ca-prinsp-head .ca-detailsbtn'));
          return until(function () { return q('.ca-prinsp .ca-tree'); }, 3000, 'the tree, with the details open');
        })
        .then(function (tree) {
          // In the tree, a worktree on another machine says so too, and its sessions, which open
          // nowhere from here, have no arrow to open them, though they keep its room.
          var places = all('[role="treeitem"][aria-expanded]', tree);
          names.forEach(function (name) {
            var marked = places.filter(function (p) { return on(q('.ca-tree-label', p), name) && !!q('.ca-tree-label .ca-devbadge', p); });
            check('its tree marks the worktrees on ' + name + ' with its badge', marked.length > 0, all('.ca-tree-label', tree).map(function (l) { return l.textContent; }).join(' | '));
          });
          var away = places.filter(function (p) { return elsewhere(q('.ca-tree-label', p)); }).flatMap(function (p) { return all('.ca-psess-open', p); });
          check('and offers no open arrow on their sessions', away.length === there.length && away.every(function (a) { return a.disabled && getComputedStyle(a).visibility === 'hidden' && a.getBoundingClientRect().width > 0; }), away.length + ' arrows, ' + there.length + ' sessions');
          var here = places.filter(function (p) { return !elsewhere(q('.ca-tree-label', p)); }).flatMap(function (p) { return all('.ca-psess-open', p); });
          var hidden = here.filter(function (a) { return a.disabled; }).length;
          check('as this Mac’s sessions keep theirs, but for one with nowhere to open', hidden === shut, hidden + ' of ' + here.length + ' hidden, ' + shut + ' with nowhere to open');
        });
    })
    .then(done)
    .catch(stopped);
})();
