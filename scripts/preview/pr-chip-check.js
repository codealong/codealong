// Checks that a worktree's PR opens on GitHub from the rows and cards that show it. In the
// Sessions and Worktrees views, where a click on a row is the row's: a click on a row's PR chip
// selects the row and opens nothing, a double-click on it opens the PR once and not the session
// too, and the row's right-click menu has Open PR. On a session's or worktree's card pinned to the desktop (given a PR and an app
// if its worktree has none): the card shows the PR, a click on it opens the PR without showing
// the peek or opening the card's session, and the card's menu has Open PR, and on a worktree's
// Open preview.
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

  if (window.__CODEALONG_DESKTOP__) {
    var pr = { number: 4242, title: 'A PR for the check', url: 'https://github.com/example/app/pull/4242', state: 'open', checks: { passed: 3, failed: 0, pending: 0, failing: [] }, merge: null, updatedAt: Date.now(), mergedAt: null, closedAt: null };
    var preview = { id: 'l|preview', icon: 'world', label: 'Open preview', detail: 'app.test', kbd: null, disabled: false, group: 'lane' };
    var kind = null;
    var laneId = null;
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
        window.__emit('desktop-feed', Object.assign({}, f, { lanes: lanes }));
        return wait(200);
      })
      .then(function () {
        var chip = document.querySelector('.ca-desk .ca-cv-card .ca-pr');
        check("a " + kind + "'s card shows its worktree's PR", !!chip && number(chip) === String(pr.number), chip && chip.textContent);
        var peeks = calls('peek_open').length;
        var opens = calls('open_from_bar').length;
        click(chip, 1);
        var o = last('open_url');
        check('a click on it opens the PR', calls('open_url').length === 1 && o.url === pr.url, JSON.stringify(o));
        return wait(500).then(function () {
          check('and not the peek', calls('peek_open').length === peeks, String(calls('peek_open').length));
          twice(chip);
          return wait(500);
        }).then(function () {
          check('a double-click on it opens it once', calls('open_url').length === 2, String(calls('open_url').length));
          check('and not the ' + kind, calls('open_from_bar').length === opens && calls('show_main').length === 0, String(calls('open_from_bar').length));
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

  // On a list's row the chip is the row's to click: a click selects the row and opens nothing,
  // and a double-click opens the PR in place of the row's session (primitives.tsx chipHandlers).
  var chipRow = function () {
    var chip = document.querySelector('.ca-board-list [role="option"]:not([aria-selected="true"]) .ca-pr');
    return chip ? { chip: chip, row: chip.closest('[role="option"]') } : null;
  };
  var selected = function () { var s = document.querySelector('.ca-board-list [role="option"][aria-selected="true"]'); return s && s.id; };
  var hit = null;
  var n = null;
  // The row and its chip as the page has them now: selecting the row renders it again.
  var row = function () { return document.getElementById(hit.row.id); };
  var chip = function () { return row().querySelector('.ca-pr'); };
  say('pr chip check: waiting for a row with a PR');
  until(function () { return !!chipRow(); }, 20000)
    // What the window does as the board first lands goes first: until its effects have run, the
    // top row is taken as the one picked (App.tsx `selected`), over a click that came in between.
    .then(function () { return wait(500); })
    .then(function () {
      hit = chipRow();
      n = number(hit.chip);
      var opens = calls('open_target').length;
      click(hit.chip, 1);
      return wait(100).then(function () {
        check("a click on a row's PR chip selects the row", selected() === hit.row.id, String(selected()));
        check('and opens nothing', calls('open_url').length === 0 && calls('open_target').length === opens, JSON.stringify(last('open_url') || last('open_target')));
        check('its tooltip says a double-click opens it', /Double-click to open it on GitHub/.test(chip().title), chip().title);
      });
    })
    .then(function () {
      var opens = calls('open_target').length;
      twice(chip());
      return wait(100).then(function () {
        var o = last('open_url');
        check('a double-click on it opens the PR once', calls('open_url').length === 1 && new RegExp('/pull/' + n + '$').test(o.url), JSON.stringify(sent('open_url')));
        check('and not the session', calls('open_target').length === opens, String(calls('open_target').length));
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
      done();
    })
    .catch(stopped);
})();
