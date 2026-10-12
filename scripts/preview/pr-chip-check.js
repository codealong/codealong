// Checks a worktree's PR chip on the rows and cards that show it: a click shows the PR in
// Codealong, once no second click has come, and a double-click opens it on GitHub. In the
// Sessions and Worktrees views, where a click on a row is the row's: its tooltip says so; a
// double-click on a row's PR chip opens the PR once, and not the session, nor selects the row;
// the row's right-click menu has Open PR; Enter on the chip with the keyboard is the chip's, not
// the window's; a click on the chip opens nothing at once, then shows the PR in PRs, selected, or
// opens it on GitHub when the board doesn't list it (the project's latest worktrees are given a
// PR if they have none: one the board lists in their project where there is one). On a
// session's or worktree's card pinned to the desktop (given a PR and an app if its worktree has
// none): the card shows the PR, a click on it asks the main window to show it, by the board's
// key, without the peek or opening the card's session, a double-click opens it on GitHub once, a
// click opens on GitHub a PR the board doesn't list, and the card's menu has Open PR, and on a
// worktree's Open preview.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-chip-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/pr-chip-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/pr-chip-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PIN=worktree scripts/preview.sh desktop '' scripts/preview/pr-chip-check.js
(function () {
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var find = function (items, text) { return items.filter(function (i) { return i.text === text; })[0]; };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  // A click as the n-th of a run, as WebKit counts them.
  var click = function (el, n) { press(el); el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: n })); };
  var twice = function (el) {
    click(el, 1);
    click(el, 2);
    el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, detail: 2 }));
  };
  // The chip's number, before its checks' count.
  var number = function (chip) { return (chip.querySelector(':scope > span').textContent.match(/#(\d+)/) || [])[1]; };
  var SAYS = /Click to show it in Codealong, double-click to open it on GitHub\./;

  if (window.__CODEALONG_DESKTOP__) {
    var pr = { number: 4242, title: 'A PR for the check', url: 'https://github.com/example/app/pull/4242', state: 'open', checks: { passed: 3, failed: 0, pending: 0, failing: [] }, merge: null, updatedAt: Date.now(), mergedAt: null, closedAt: null };
    var preview = { id: 'l|preview', icon: 'world', label: 'Open preview', detail: 'app.test', kbd: null, disabled: false, group: 'lane' };
    var kind = null;
    var laneId = null;
    var feed = null;
    var key = null;
    var chip = function () { return document.querySelector('.ca-desk .ca-cv-card .ca-pr'); };
    say('pr chip check: waiting for the card');
    until(function () { return document.querySelector('.ca-desk .ca-cv-card.is-session, .ca-desk .ca-cv-card.is-worktree'); }, 20000)
      .then(function () { return window.__TAURI_INTERNALS__.invoke('desktop_feed'); })
      .then(function (f) {
        kind = f.pin.ref;
        // The card's worktree, given a PR and an app if it has none.
        var lanes = f.lanes.map(function (l, i) {
          if (i > 0) return l;
          var targets = l.targets.some(function (t) { return t.id === 'l|preview'; }) ? l.targets : l.targets.concat([preview]);
          return Object.assign({}, l, { pr: l.pr || pr, targets: targets });
        });
        pr = lanes[0].pr;
        laneId = lanes[0].id;
        // The PR as the board lists it, which the feed carries then (desktop.rs `listed_pr`):
        // the board's own, else one made for it under a key of its own.
        var listed = (f.prs || []).filter(function (p) { return p.url === pr.url; })[0] || Object.assign({}, pr, { key: 'Example/App#' + pr.number, repoSlug: 'Example/App', repo: 'App' });
        key = listed.key;
        feed = Object.assign({}, f, { lanes: lanes });
        window.__emit('desktop-feed', Object.assign({}, feed, { prs: [listed] }));
        return wait(200);
      })
      .then(function () {
        check("a " + kind + "'s card shows its worktree's PR", !!chip() && number(chip()) === String(pr.number), chip() && chip().textContent);
        check('its tooltip says what a click and a double-click do', SAYS.test(chip().title), chip().title);
        var peeks = calls('peek_open').length;
        var opens = calls('open_from_bar').length;
        click(chip(), 1);
        check('a click on it waits for a second', calls('open_url').length === 0 && calls('show_main').length === 0, JSON.stringify(window.__calls.slice(-2)));
        return wait(500).then(function () {
          var s = last('show_main');
          check('then shows the PR in Codealong, by the board’s key', calls('show_main').length === 1 && !!s && !!s.reveal && s.reveal.kind === 'pr' && s.reveal.key === key, JSON.stringify(s));
          check('and opens nothing', calls('open_url').length === 0, JSON.stringify(last('open_url')));
          check('nor the peek', calls('peek_open').length === peeks, String(calls('peek_open').length));
          twice(chip());
          return wait(500);
        }).then(function () {
          var o = last('open_url');
          check('a double-click on it opens it on GitHub once', calls('open_url').length === 1 && o.url === pr.url, JSON.stringify(sent('open_url')));
          check('and not the ' + kind + ', nor shows it again', calls('open_from_bar').length === opens && calls('show_main').length === 1, String(calls('open_from_bar').length) + ', ' + String(calls('show_main').length));
          // A PR the board doesn't list has no panel in Codealong: a click opens it on GitHub.
          window.__emit('desktop-feed', Object.assign({}, feed, { prs: [] }));
          return wait(200);
        }).then(function () {
          click(chip(), 1);
          return wait(500);
        }).then(function () {
          check('a click on a PR the board doesn’t list opens it on GitHub', calls('open_url').length === 2 && last('open_url').url === pr.url && calls('show_main').length === 1, JSON.stringify(sent('open_url')) + ', ' + String(calls('show_main').length));
          rightClick(document.querySelector('.ca-desk-card'));
          return wait(200);
        });
      })
      .then(function () {
        var menu = lastMenu();
        var item = find(menu, 'Open PR #' + pr.number + ' on GitHub');
        check("the card's menu opens the PR", !!item, texts(menu).join('|'));
        pick(item);
        check('on GitHub', calls('open_url').length === 3 && last('open_url').url === pr.url, JSON.stringify(last('open_url')));
        if (kind === 'worktree') {
          var p = find(menu, 'Open preview');
          check("a worktree's menu opens its app", !!p, texts(menu).join('|'));
          pick(p);
          var o = last('open_from_bar');
          check('in the browser', o && o.laneId === laneId && o.targetId === 'l|preview', JSON.stringify(o));
        }
        done();
      })
      .catch(stopped);
    return;
  }

  // The shown project's three latest worktrees with sessions get a PR each if they have none, so
  // there's a row with a PR that isn't the one selected on any Mac: one the board lists in their
  // project where there is one, so a click has its panel to show, else a made-up one. This runs
  // before the app's script, so the first board the page reads has them. Each lane's PR is kept,
  // and the board's key for each PR it lists.
  var prOf = {};
  var listed = {};
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      // The project the window shows, as boardLists.ts `currentProject` picks it.
      var picked = localStorage.getItem('codealong.project');
      var project = picked === 'All projects' || b.projects.some(function (x) { return x.name === picked; }) ? picked : (b.projects.find(function (x) { return x.name !== 'Elsewhere'; }) || { name: 'All projects' }).name;
      b.prs.forEach(function (x) { listed[x.url] = x.key; });
      b.lanes
        .filter(function (l) { return !l.hidden && !l.device && (project === 'All projects' || l.project === project) && l.sessions.some(function (x) { return !x.visitor && !x.archived; }); })
        .sort(function (x, y) { return y.lastActivity - x.lastActivity; })
        .slice(0, 3)
        .forEach(function (l, i) {
          if (l.pr) return;
          var own = b.prs.filter(function (x) { return x.project === l.project; })[i];
          var number = 4242 + i;
          l.pr = own
            ? { number: own.number, title: own.title, url: own.url, state: own.state, checks: own.checks, merge: own.merge, updatedAt: own.updatedAt, mergedAt: own.mergedAt, closedAt: own.closedAt }
            : { number: number, title: 'A PR for the check', url: 'https://github.com/example/app/pull/' + number, state: 'open', checks: { passed: 3, failed: 0, pending: 0, failing: [] }, merge: null, updatedAt: Date.now(), mergedAt: null, closedAt: null };
        });
      b.lanes.forEach(function (l) { if (l.pr) prOf[l.id] = l.pr.url; });
      return b;
    });
  };

  // On a list's row the chip takes its own clicks (primitives.tsx chipHandlers' `twice`): a
  // click shows the PR in PRs once no second has come, and a double-click opens it on GitHub, in
  // place of the row's session; neither selects the row.
  // The row a chip is on: its own, or on a worktree's row with agents' lines (a group), where the
  // chip is on the line under them, its first agent's.
  var rowOf = function (chip) {
    var group = chip.closest('[role="group"]');
    return chip.closest('[role="option"]') || (group && group.querySelector('[role="option"]'));
  };
  var chipRow = function () {
    var chips = all('.ca-board-list .ca-pr');
    for (var i = 0; i < chips.length; i++) {
      var r = rowOf(chips[i]);
      if (r && r.getAttribute('aria-selected') !== 'true') return { chip: chips[i], row: r };
    }
    return null;
  };
  var selected = function () { var s = document.querySelector('.ca-board-list [role="option"][aria-selected="true"]'); return s && s.id; };
  var view = function () { return document.querySelector('[role="listbox"]').getAttribute('aria-label'); };
  var hit = null;
  var n = null;
  var url = null;
  var from = null;
  var before = null;
  // The row and its chip as the page has them now.
  var row = function () { return document.getElementById(hit.row.id); };
  var chip = function () { return (row().closest('[role="group"]') || row()).querySelector('.ca-pr'); };
  say('pr chip check: waiting for a row with a PR');
  until(function () { return !!chipRow(); }, 20000)
    // What the window does as the board first lands goes first: until its effects have run, the
    // top row is taken as the one picked (App.tsx `selected`), over a click that came in between.
    .then(function () { return wait(500); })
    .then(function () {
      hit = chipRow();
      n = number(hit.chip);
      url = Object.keys(prOf).map(function (k) { return prOf[k]; }).filter(function (u) { return new RegExp('/pull/' + n + '$').test(u); })[0];
      from = view();
      before = selected();
      check("a row's PR chip says what a click and a double-click do", SAYS.test(hit.chip.title), hit.chip.title);
      var opens = calls('open_target').length;
      twice(hit.chip);
      return wait(400).then(function () {
        var o = last('open_url');
        check('a double-click on it opens the PR once', calls('open_url').length === 1 && new RegExp('/pull/' + n + '$').test(o.url), JSON.stringify(sent('open_url')));
        check('and not the session', calls('open_target').length === opens, String(calls('open_target').length));
        check('nor selects the row or leaves the view', selected() === before && view() === from, String(selected()) + ' in ' + view());
        rightClick(row());
        return wait(200);
      });
    })
    .then(function () {
      var menu = lastMenu();
      var item = find(menu, 'Open PR #' + n + ' on GitHub');
      check("the row's menu opens the PR", !!item, texts(menu).join('|'));
      pick(item);
      return wait(50);
    })
    .then(function () {
      check('on GitHub', calls('open_url').length === 2 && new RegExp('/pull/' + n + '$').test(last('open_url').url), JSON.stringify(last('open_url')));
      var opens = calls('open_target').length;
      // Enter on the chip, reached by Tab, is the chip's: the window doesn't open the selected row.
      var c = chip();
      c.focus();
      var enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
      c.dispatchEvent(enter);
      check('Enter on the chip with the keyboard is its own, not the selected row’s', !enter.defaultPrevented && calls('open_target').length === opens, String(calls('open_target').length));
      c.blur();
      click(chip(), 1);
      return wait(100)
        .then(function () {
          check('a click on the chip does nothing at once: a second may come', calls('open_url').length === 2 && view() === from && selected() === before, String(selected()) + ' in ' + view());
          return wait(400);
        })
        .then(function () {
          check('nor opens the session', calls('open_target').length === opens, String(calls('open_target').length));
          if (listed[url]) {
            check('then shows the PR in PRs, selected', view() === 'Pull requests' && selected() === 'row-pr:' + listed[url], String(selected()) + ' in ' + view());
            check('and opens nothing', calls('open_url').length === 2, JSON.stringify(last('open_url')));
          } else {
            note('#' + n + ' is a PR the board doesn’t list');
            check('then opens it on GitHub, having no panel to show', calls('open_url').length === 3 && last('open_url').url === url && view() === from, JSON.stringify(last('open_url')));
          }
        });
    })
    .then(done)
    .catch(stopped);
})();
