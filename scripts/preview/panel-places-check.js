// The worktrees and sessions behind a PR or a todo, in its panel (PanelSession.tsx `PanelPlaces`).
// In PRs: the header is short, so the tabs start near the panel's top; under it the PR's worktrees
// and sessions are chips, a worktree's then its sessions'; the chevron beside the title opens the
// details, which list them as a tree, with a row for each session, and stay open for the next PR
// until it's pressed again; the button beside it, and ⇧⌘F, expand the panel to the whole window
// and back. In Todo: the todo's are chips too, and its chevron beside Link makes them a tree.
// A session's chip, there and on the PR's or todo's row (on two lines), says its title, with its
// account on hover.
//   scripts/preview.sh prs '' scripts/preview/panel-places-check.js
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/panel-places-check.js
(function () {
  localStorage.removeItem('codealong.prDetailsOpen');
  localStorage.removeItem('codealong.prPanelMax');
  localStorage.removeItem('codealong.todoPlacesOpen');
  localStorage.setItem('codealong.prRows', 'two');
  localStorage.setItem('codealong.todoRows', 'two');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var key = function (init) { window.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var shown = function (el) { return !!el && el.getClientRects().length > 0; };
  // The rows whose PR has sessions on this Mac, which its panel lists.
  var prRows = function () { return all('.ca-prrow').filter(function (r) { return !!q('.ca-prrow-agents', r); }); };
  // Whether each session chip says its title, and on hover its title, then its account (harness).
  var named = function (chips) {
    return chips.length > 0 && chips.every(function (c) {
      var title = q('.ca-sesschip-title', c);
      var lines = (c.getAttribute('title') || '').split('\n');
      return c.classList.contains('ca-sesschip') && !!title && title.textContent !== '' && lines[0] === title.textContent && /^\S+ \(.+\) · /.test(lines[1] || '');
    });
  };
  var sayChips = function (chips) { return chips.map(function (c) { return c.textContent + ' [' + (c.getAttribute('title') || '').split('\n').slice(0, 2).join(' / ') + ']'; }).join(' | '); };

  var prs = function () {
    var rows = prRows();
    if (rows.length === 0) return stopped('no PR on this board has a session on this Mac');
    var rowChips = all('.ca-prrow-agents > .ca-chip:not(.ca-chip--more)', rows[0]);
    check('a PR’s row names its sessions by their titles', named(rowChips), sayChips(rowChips));
    var fit = q('.ca-prrow-fit', rows[0]).getBoundingClientRect();
    check('and they keep inside the row’s line', rowChips.every(function (c) { return c.getBoundingClientRect().right <= fit.right + 1; }));
    press(rows[0]);
    return until(function () { return q('.ca-prinsp-head .ca-places'); }, 5000, 'the PR panel’s chips')
      .then(function (places) {
        var chips = all('.ca-chip', places);
        check('its worktree comes first, as a chip', chips.length > 0 && chips[0].classList.contains('ca-wtchip'), chips.map(function (c) { return c.className; }).join(' | '));
        check('then its sessions, as chips', chips.length >= 2, String(chips.length));
        check('no tree while its details are closed', !q('.ca-prinsp .ca-tree'));
        var panel = q('.ca-prpanel').getBoundingClientRect();
        var tabs = q('.ca-prtabs').getBoundingClientRect();
        check('the tabs start near the panel’s top', tabs.top - panel.top < 360, Math.round(tabs.top - panel.top) + 'px down');
        var sessionChips = all('.ca-prinsp-head .ca-places .ca-chip:not(.ca-wtchip)');
        check('its sessions’ chips say their titles, with the account on hover', named(sessionChips), sayChips(sessionChips));
        var sessions = sessionChips.length;
        press(q('.ca-prinsp-head .ca-detailsbtn'));
        return until(function () { return q('.ca-prinsp .ca-tree'); }, 3000, 'the tree, with the details open').then(function () {
          check('the details list each session as a row', all('.ca-prinsp .ca-tree .ca-psess').length === sessions, all('.ca-prinsp .ca-tree .ca-psess').length + ' rows, ' + sessions + ' chips');
          check('in place of the chips', !q('.ca-prinsp-head .ca-places'));
          check('the button says they’re open', q('.ca-prinsp-head .ca-detailsbtn').getAttribute('aria-expanded') === 'true');
        });
      })
      .then(function () {
        var next = all('.ca-prrow').filter(function (r) { return !r.classList.contains('is-selected') && r.getAttribute('aria-selected') !== 'true'; })[0];
        if (!next) {
          note('no second PR to go on to');
          return null;
        }
        press(next);
        return wait(300).then(function () {
          check('the next PR opens with its details open', !!q('.ca-prinsp-head.is-open'));
          press(q('.ca-prinsp-head .ca-detailsbtn'));
          return wait(100);
        });
      })
      .then(function () {
        check('closing them closes them', !q('.ca-prinsp-head.is-open'));
        var max = q('.ca-prinsp-tools [aria-label="Expand to the window"]');
        check('PRs offers the whole window', !!max);
        if (!max) return null;
        press(max);
        return until(function () { return q('.ca-win-body.is-panel-max'); }, 3000, 'the panel taking the window').then(function () {
          var w = q('.ca-prpanel').getBoundingClientRect().width;
          check('the panel takes the window’s width', w > window.innerWidth - 4, Math.round(w) + ' of ' + window.innerWidth);
          check('the list and the sidebar step aside', !shown(q('.ca-board')) && !shown(q('.ca-side')));
          key({ key: 'F', code: 'KeyF', metaKey: true, shiftKey: true });
          return wait(100);
        }).then(function () {
          check('⇧⌘F gives the list back', !q('.ca-win-body.is-panel-max') && shown(q('.ca-board')));
          key({ key: 'F', code: 'KeyF', metaKey: true, shiftKey: true });
          return wait(100);
        }).then(function () {
          check('and takes the window again', !!q('.ca-win-body.is-panel-max'));
          press(q('.ca-prinsp-tools [aria-label="Show the list"]'));
          return wait(100);
        }).then(function () {
          check('its button gives the list back', !q('.ca-win-body.is-panel-max'));
          // At the narrowest the panel gets, its header still keeps to its width.
          q('.ca-win-body').style.setProperty('--panel-w', '300px');
          return wait(100);
        }).then(function () {
          var body = q('.ca-prpanel-body');
          var meta = q('.ca-prinsp-meta');
          check('the panel is 300px wide', Math.abs(q('.ca-prpanel').getBoundingClientRect().width - 300) < 2, String(q('.ca-prpanel').getBoundingClientRect().width));
          check('at 300px nothing in it scrolls sideways', body.scrollWidth <= body.clientWidth + 1, body.scrollWidth + ' in ' + body.clientWidth);
          var last = meta.lastElementChild.getBoundingClientRect().right;
          check('and its one line of where it comes from fits, cut short rather than cut off', meta.getBoundingClientRect().right <= body.getBoundingClientRect().right + 1 && last <= meta.getBoundingClientRect().right + 1, Math.round(last) + ' in ' + Math.round(meta.getBoundingClientRect().right));
        });
      });
  };

  var todos = function () {
    var rowChips = all('.ca-todorow .ca-chips > .ca-chip').filter(function (c) { return !!q('.ca-sesschip-title', c) || c.classList.contains('ca-sesschip'); });
    if (rowChips.length > 0) check('a todo’s row names its sessions by their titles', named(rowChips), sayChips(rowChips));
    else note('no todo row here has a session');
    press(q('.ca-todorow'));
    return until(function () { return q('.ca-todoinsp .ca-places') || q('.ca-todoinsp .ca-empty'); }, 5000, 'the todo panel’s worktrees and sessions')
      .then(function () {
        var places = q('.ca-todoinsp .ca-places');
        if (!places) return stopped('the sample’s first todo has no worktree or session on this board');
        var chips = all('.ca-chip', places);
        check('the todo’s worktree is a chip, then its sessions', chips.length >= 2 && chips[0].classList.contains('ca-wtchip'), chips.map(function (c) { return c.textContent; }).join(' | '));
        var primary = chips.filter(function (c) { return !!q('.ca-places-primary', c); });
        check('its primary session’s chip says so', primary.length === 1, String(primary.length));
        check('its sessions’ chips say their titles, with the account on hover', named(chips.filter(function (c) { return !c.classList.contains('ca-wtchip'); })), sayChips(chips));
        var sessions = chips.filter(function (c) { return !c.classList.contains('ca-wtchip'); }).length;
        press(q('.ca-todoinsp .ca-detailsbtn'));
        return until(function () { return q('.ca-todoinsp .ca-tree'); }, 3000, 'the todo’s tree').then(function () {
          var rows = all('.ca-todoinsp .ca-psess').length;
          check('its chevron makes them a tree', rows === sessions && !q('.ca-todoinsp .ca-places'), rows + ' rows, ' + sessions + ' chips');
          check('the primary one tagged there', all('.ca-todoinsp .ca-psess-primary').length === 1);
        });
      });
  };

  say('panel places check: waiting for the list');
  until(function () { return prRows().length > 0 || q('.ca-todorow') || null; }, 20000, 'a PR with sessions, or a todo')
    .then(function () { return prRows().length > 0 ? prs() : todos(); })
    .then(done)
    .catch(stopped);
})();
