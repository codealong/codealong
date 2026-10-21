// Checks the Sessions list (SessionsView.tsx, App.tsx): Pinned first, and a
// drag still reorders them; the rest under the headings the other views have, by status or by
// the day they started. Sort by, icon buttons in the bar over the list, switches between status
// and start and remembers it. The bar stays at the top as the list scrolls; its tools are each
// view's own, so PRs has no Group by; the sidebar has no Sort by.
//   scripts/preview.sh sessions '' scripts/preview/session-sort-check.js
// It pins three of the shown project's latest sessions itself, in place of yours.
(function () {
  // This runs before the app's script, so the first board the page reads has these pins.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      // The project the window shows, as boardLists.ts `currentProject` picks it.
      var picked = localStorage.getItem('codealong.project');
      var project = picked === 'All projects' || b.projects.some(function (x) { return x.name === picked; }) ? picked : (b.projects.find(function (x) { return x.name !== 'Elsewhere'; }) || { name: 'All projects' }).name;
      var shown = [];
      b.lanes.forEach(function (l) {
        if (l.hidden || l.device || (project !== 'All projects' && l.project !== project)) return;
        l.sessions.forEach(function (x) { if (!x.visitor && !x.archived) shown.push(x); });
      });
      shown.sort(function (x, y) { return y.lastActivity - x.lastActivity; });
      b.pinned = shown.slice(0, 3).map(function (x) { return x.key; });
      return b;
    });
  };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessionlist .ca-lane')); };
  var ids = function () { return rows().map(function (r) { return r.id; }).join(','); };
  var sortButton = function (label) {
    return document.querySelector('.ca-listtools button[aria-label="' + label + '"]');
  };
  // Each heading's words without its count, in the list's order.
  var headings = function () {
    return Array.prototype.map.call(document.querySelectorAll('.ca-sessionlist .ca-bay'), function (h) { return h.querySelector('span').textContent; });
  };
  var STATUS_HEADS = ['Pinned', 'Needs you', 'Your turn', 'Working', 'Snoozed', 'Today', 'Yesterday', 'Last 7 days', 'Older'];
  var DAY_HEADS = ['Pinned', 'Today', 'Yesterday', 'Last 7 days', 'Older'];
  // In that order, each once, none left out but those with nothing under them.
  var inOrderOf = function (heads, order) {
    var at = heads.map(function (h) { return order.indexOf(h); });
    return at.every(function (i, n) { return i >= 0 && (n === 0 || i > at[n - 1]); });
  };
  var nav = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side .ca-nav'), function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === label; })[0];
  };
  var pointer = function (target, type, y) {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, clientX: 400, clientY: y, pointerId: 1, isPrimary: true }));
  };
  say('session sort check: waiting for the board');
  until(function () { return rows().length > 0; }, 20000)
    .then(function () {
      // Sessions' tools, in the strip over the list (ListBar.tsx).
      var bar = document.querySelector('.ca-listbar');
      check('the bar is over the list, outside it', !!bar && !!bar.querySelector('.ca-listtools') && !bar.closest('[role="listbox"]') && bar.getBoundingClientRect().bottom <= rows()[0].getBoundingClientRect().top + 1);
      var sideLabels = Array.prototype.map.call(document.querySelectorAll('.ca-side .ca-side-label'), function (l) { return l.textContent; });
      check('the sidebar has no Sort by or Rows', sideLabels.indexOf('Sort by') < 0 && sideLabels.indexOf('Rows') < 0, sideLabels.join(' | '));
      // Stuck to the top of the board as the list scrolls: layout places it, so no frame is needed.
      var board = document.querySelector('.ca-board');
      board.scrollTop = 400;
      var stuck = board.scrollTop === 0 || Math.abs(bar.getBoundingClientRect().top - board.getBoundingClientRect().top) < 1;
      check('the bar stays at the top as the list scrolls', stuck, 'scrolled ' + board.scrollTop + ', bar ' + Math.round(bar.getBoundingClientRect().top) + ', board ' + Math.round(board.getBoundingClientRect().top));
      // A row scrolled to from below, as the keyboard's ↑ scrolls to it (App.tsx), shows whole under the bar.
      var second = rows()[1];
      if (board.scrollTop > 0 && second) {
        second.scrollIntoView({ block: 'nearest' });
        check('a row scrolled to shows under the bar, not behind it', second.getBoundingClientRect().top >= bar.getBoundingClientRect().bottom - 1, 'row ' + Math.round(second.getBoundingClientRect().top) + ', bar ends ' + Math.round(bar.getBoundingClientRect().bottom));
      }
      board.scrollTop = 0;
      var sortIcons = Array.prototype.map.call(document.querySelectorAll('.ca-listtools button'), function (b) { return !!b.querySelector('svg') && !b.textContent.trim() && !!b.getAttribute('aria-label') && !!b.title; });
      // Sort by's two, Group by's three and Rows' two.
      check('the bar’s buttons are icons, each named and with a tooltip', sortIcons.length === 7 && sortIcons.every(Boolean), sortIcons.join(','));
      check('headings by status, Pinned first, as the other views head them', headings()[0] === 'Pinned' && inOrderOf(headings(), STATUS_HEADS), headings().join(' | '));
      var counts = Array.prototype.map.call(document.querySelectorAll('.ca-sessionlist .ca-bayblock'), function (b) { return Number(b.querySelector('.ca-bay-count').textContent) === b.querySelectorAll('.ca-lane').length; });
      check('each heading counts the rows under it', counts.length > 1 && counts.every(Boolean));
      var all = rows();
      var pinned = all.filter(function (r) { return r.classList.contains('is-pinned'); });
      check('pinned rows first', pinned.length >= 2 && pinned.every(function (r, i) { return all[i] === r; }), pinned.length + ' pinned');
      var after = all[pinned.length];
      check('the rest are under headings of their own', !after || after.closest('.ca-bayblock') !== pinned[0].closest('.ca-bayblock'));

      // Drag the first pinned row below the last: it becomes the last pin.
      var first = pinned[0];
      var last = pinned[pinned.length - 1].getBoundingClientRect();
      // The calls from here on. The stub finds the page's listeners in __calls, so it's never emptied.
      var from = window.__calls.length;
      pointer(first, 'pointerdown', first.getBoundingClientRect().top + 10);
      pointer(window, 'pointermove', last.top + 20);
      pointer(window, 'pointermove', last.bottom - 2);
      pointer(window, 'pointerup', last.bottom - 2);
      var saved = window.__calls.slice(from).filter(function (c) { return c[0] === 'set_pinned'; })[0];
      var keys = saved ? saved[1].keys : [];
      var moved = first.id.split('::').pop();
      var shownKeys = pinned.map(function (r) { return r.id.split('::').pop(); });
      var order = keys.filter(function (k) { return shownKeys.indexOf(k) >= 0; });
      check('a drag reorders the pins', order[order.length - 1] === moved, JSON.stringify(order));
      // Sort by is clicked once the list shows the new order. A click in the same turn as the
      // drop is taken for the click the drag ends in, and swallowed (useRowDrag.tsx); nobody can
      // click that fast by hand.
      return until(function () {
        var now = rows().filter(function (r) { return r.classList.contains('is-pinned'); });
        return now.length === pinned.length && now[now.length - 1].id === first.id;
      }, 2000, 'the list showing the dragged pin last');
    })
    .then(function () {
      var before = ids();
      check('sorted by status to begin with', sortButton('Status') && sortButton('Status').classList.contains('is-on'));
      sortButton('Started').click();
      return until(function () { return ids() !== before; }, 2000).then(function () {
        check('Started reorders the list', true);
        check('under the days they started', inOrderOf(headings(), DAY_HEADS), headings().join(' | '));
        check('and is remembered', localStorage.getItem('codealong.sessionSort') === 'started');
        var nowPinned = rows().filter(function (r) { return r.classList.contains('is-pinned'); });
        check('pinned rows stay first', nowPinned.every(function (r, i) { return rows()[i] === r; }));
        sortButton('Status').click();
        return until(function () { return localStorage.getItem('codealong.sessionSort') === 'status'; }, 2000);
      });
    })
    .then(function () {
      nav('PRs').click();
      return until(function () { return !document.querySelector('.ca-sessionlist'); }, 2000);
    })
    .then(function () {
      // Its own Group by, apart from Sessions': grouped there, PRs starts with none.
      var group = document.querySelector('.ca-listtools [aria-label="Group by"] [aria-checked="true"]');
      check('PRs has its own tools in the bar, its Group by its own', !!document.querySelector('.ca-listtools') && !!group && group.getAttribute('aria-label') === 'No groups', group ? group.getAttribute('aria-label') : 'no Group by');
    })
    .then(done)
    .catch(stopped);
})();
