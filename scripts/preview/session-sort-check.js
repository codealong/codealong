// Checks the Sessions list (SessionsView.tsx, App.tsx): one list with no headings, pinned rows
// first, each with its pin, and a drag still reorders them; the sidebar's Sort by switches
// between status and start, remembers it, and shows only in Sessions.
//   scripts/preview.sh sessions '' scripts/preview/session-sort-check.js
// It needs a project with two pinned sessions or more.
(function () {
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessionlist .ca-lane')); };
  var ids = function () { return rows().map(function (r) { return r.id; }).join(','); };
  var sortButton = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side-sorts button'), function (b) { return b.textContent === label; })[0];
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
      var headings = Array.prototype.filter.call(document.querySelectorAll('.ca-board .ca-bay'), function (h) { return !h.closest('.ca-archived'); });
      check('no headings over the list', headings.length === 0, headings.map(function (h) { return h.textContent; }).join(' | '));
      var all = rows();
      var pinned = all.filter(function (r) { return r.classList.contains('is-pinned'); });
      check('pinned rows first', pinned.length >= 2 && pinned.every(function (r, i) { return all[i] === r; }), pinned.length + ' pinned');
      check('each pinned row has its pin', pinned.every(function (r) { return !!r.querySelector('.ca-pinmark'); }) && all.slice(pinned.length).every(function (r) { return !r.querySelector('.ca-pinmark'); }));
      var after = all[pinned.length];
      check('the rest start a little apart', !after || parseFloat(getComputedStyle(after).marginTop) > 0);

      // Drag the first pinned row below the last: it becomes the last pin.
      var first = pinned[0];
      var last = pinned[pinned.length - 1].getBoundingClientRect();
      window.__calls.length = 0;
      pointer(first, 'pointerdown', first.getBoundingClientRect().top + 10);
      pointer(window, 'pointermove', last.top + 20);
      pointer(window, 'pointermove', last.bottom - 2);
      pointer(window, 'pointerup', last.bottom - 2);
      var saved = window.__calls.filter(function (c) { return c[0] === 'set_pinned'; })[0];
      var keys = saved ? saved[1].keys : [];
      var moved = first.id.split('::').pop();
      var shownKeys = pinned.map(function (r) { return r.id.split('::').pop(); });
      var order = keys.filter(function (k) { return shownKeys.indexOf(k) >= 0; });
      check('a drag reorders the pins', order[order.length - 1] === moved, JSON.stringify(order));

      var before = ids();
      check('sorted by status to begin with', sortButton('Status') && sortButton('Status').classList.contains('is-on'));
      sortButton('Started').click();
      return until(function () { return ids() !== before; }, 2000).then(function () {
        check('Started reorders the list', true);
        check('and is remembered', localStorage.getItem('codealong.sessionSort') === 'started');
        var nowPinned = rows().filter(function (r) { return r.classList.contains('is-pinned'); });
        check('pinned rows stay first', nowPinned.every(function (r, i) { return rows()[i] === r; }));
        sortButton('Status').click();
        return until(function () { return localStorage.getItem('codealong.sessionSort') === 'status'; }, 2000);
      });
    })
    .then(function () {
      nav('Worktrees').click();
      return until(function () { return !document.querySelector('.ca-sessionlist'); }, 2000);
    })
    .then(function () {
      check('Sort by shows only in Sessions', !document.querySelector('.ca-side-sorts'));
    })
    .then(done)
    .catch(stopped);
})();
