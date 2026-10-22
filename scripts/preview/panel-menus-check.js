// Checks a right-click on the header of each panel of the main window that isn't a session's
// (headerMenu in primitives.tsx; a session's is header-menu-check.js): a worktree's (its heading's
// panel in Sessions grouped by worktree), a PR's and a todo's give the menu their row gives in the
// list, at the pointer, and so does a todo's title
// you aren't typing in, which then lets go of the keyboard the right-click gave it, while one
// you're typing in keeps its own menu. Several sessions picked together give the menu a picked
// row gives, the one their ⋯ drops.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh sessions '' scripts/preview/panel-menus-check.js
// A step is noted as skipped where this Mac has no PR, or fewer than two sessions.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.setItem('codealong.sessionGroup', 'worktree');
  localStorage.removeItem('codealong.age');

  var press = function (el, init) { el.dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  // A right-click as WebKit sends one: the press, then the menu's event.
  var right = function (el) {
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 2 }));
    rightClick(el, { clientX: 40, clientY: 40 });
  };
  var labels = function () { return menuLabels().join('|'); };
  // The menu a right-click on `el` gives, or '' when it gives none.
  var menuAt = function (el) {
    var n = menus();
    right(el);
    return wait(300).then(function () { return menus() > n ? labels() : ''; });
  };
  var atPointer = function () {
    var at = calls('popup_menu').pop();
    return !!at && !at[1].at;
  };
  var view = function (label) {
    var nav = all('.ca-side .ca-nav').filter(function (b) { var l = q('.ca-nav-label', b); return l && l.textContent === label; })[0];
    if (!nav) throw new Error('no ' + label + ' in the sidebar');
    nav.click();
    return wait(300);
  };
  // The menu a row gives, then the one its panel's header gives on `part` of it.
  var same = function (name, row, part) {
    var want = '';
    return menuAt(row)
      .then(function (got) {
        want = got;
        return until(part, 5000, name + '’s panel');
      })
      .then(function (el) { return menuAt(el); })
      .then(function (got) {
        check('a right-click on a ' + name + '’s header gives its row’s menu', !!want && got === want, (got || 'no menu') + ' / ' + (want || 'no menu'));
        check('at the pointer', atPointer());
      });
  };

  say('panel menus check: waiting for the board');
  until(function () { return q('.ca-grouphead.is-worktree-head'); }, 20000, 'a worktree’s heading')
    .then(function (row) {
      press(row);
      return same('worktree', row, function () { return q('.ca-insp:not(.ca-sessinsp) header .ca-insp-call'); });
    })
    .then(function () {
      return menuAt(q('.ca-insp header .ca-insp-path')).then(function (got) {
        check('and so does its folder, a button with no menu of its own', /Pin worktree|Unpin worktree/.test(got), got || 'no menu');
      });
    })
    .then(function () { return view('PRs'); })
    .then(function () {
      var row = q('.ca-prrow');
      if (!row) return void note('a PR’s header skipped: no PR');
      press(row);
      return same('PR', row, function () { return q('.ca-prinsp header .ca-prinsp-title'); });
    })
    .then(function () { return view('Todo'); })
    .then(function () { return until(function () { return q('.ca-todorow-title'); }, 5000, 'a todo'); })
    .then(function (row) {
      press(row);
      return same('todo', row, function () { return q('.ca-todoinsp-meta'); });
    })
    .then(function () {
      var want = labels();
      var title = q('.ca-todoinsp header textarea');
      if (!title) throw new Error('no title field');
      var n = menus();
      // WebKit gives the field the keyboard, and a word selected, before the menu's event.
      title.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 2 }));
      title.focus();
      rightClick(title, { clientX: 40, clientY: 40 });
      return wait(300).then(function () {
        check('its title gives the todo’s menu when you aren’t typing in it', menus() === n + 1 && labels() === want, labels());
        check('and lets go of the keyboard', document.activeElement !== title, String(document.activeElement && document.activeElement.tagName));
        title.focus();
        return menuAt(title);
      });
    })
    .then(function (got) {
      check('one you’re typing in keeps its own menu', got === '', got);
      document.activeElement.blur();
      return view('Sessions');
    })
    .then(function () {
      var rows = all('.ca-sessionlist .ca-lane.is-agent:not(.is-fold)');
      if (rows.length < 2) return void note('picked sessions skipped: fewer than two');
      press(rows[0]);
      press(rows[1], { metaKey: true });
      var want = '';
      return until(function () { return q('.ca-pick'); }, 3000, 'the pick panel')
        .then(function () { return menuAt(rows[0]); })
        .then(function (got) {
          want = got;
          return menuAt(q('.ca-pick-head'));
        })
        .then(function (got) {
          check('a right-click on picked sessions’ header gives a picked row’s menu', !!want && got === want, (got || 'no menu') + ' / ' + (want || 'no menu'));
          check('at the pointer', atPointer());
          q('.ca-pick button[aria-label="More"]').click();
          return wait(300);
        })
        .then(function () {
          check('the one ⋯ drops', labels() === want, labels());
        });
    })
    .then(done)
    .catch(stopped);
})();
