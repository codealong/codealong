// Checks the sidebar (Sidebar.tsx, App.tsx): it never scrolls sideways; statuses and devices
// toggle, several at once, and Clear unpicks them; the view saves under a name and comes back
// when picked; ⌃⌘S folds the sidebar to a rail whose filter button counts the filters on and
// opens it again; and a saved view's menu deletes it.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/sidebar-check.js
(function () {
  var nav = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side .ca-nav'), function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === label; })[0];
  };
  var chip = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side-chip'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var saved = function () { return JSON.parse(localStorage.getItem('codealong.savedViews') || '[]'); };
  var ageOn = function () { var on = document.querySelector('.ca-side-ages button.is-on'); return on && on.textContent !== 'All'; };
  var expected;
  var devRows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-side button.ca-side-devrow')); };
  var clear = function (label) {
    var head = Array.prototype.filter.call(document.querySelectorAll('.ca-side-label--row'), function (h) { return h.firstChild.textContent === label; })[0];
    return head && head.querySelector('.ca-side-clear');
  };

  say('sidebar check: waiting for the sidebar');
  until(function () { return nav('Sessions') && chip('Your turn'); }, 20000)
    .then(function () {
      var box = document.querySelector('.ca-side-scroll');
      check('the sidebar never scrolls sideways', box.scrollWidth <= box.clientWidth && getComputedStyle(box).overflowX === 'hidden', box.scrollWidth + ' in ' + box.clientWidth);
      var side = document.querySelector('.ca-side').getBoundingClientRect();
      var body = document.querySelector('.ca-win-body').getBoundingClientRect();
      check('and reaches the bottom of the window', Math.abs(side.bottom - body.bottom) < 1 && Math.abs(side.top - body.top) < 1, side.top + ' to ' + side.bottom + ' in ' + body.top + ' to ' + body.bottom);
      check('with no All devices row', !nav('All devices'));
      chip('Your turn').click();
      chip('Needs you').click();
      return wait(50);
    })
    .then(function () {
      check('two statuses pick together', chip('Your turn').getAttribute('aria-pressed') === 'true' && chip('Needs you').getAttribute('aria-pressed') === 'true');
      clear('Status').click();
      return wait(50);
    })
    .then(function () {
      check('Clear unpicks them', chip('Your turn').getAttribute('aria-pressed') === 'false' && chip('Needs you').getAttribute('aria-pressed') === 'false' && !clear('Status'));
      var rows = devRows();
      if (rows.length < 2) {
        check('devices (run with PREVIEW_DEVICES)', false, rows.length + ' device rows');
        return wait(0);
      }
      rows[0].click();
      rows[1].click();
      return wait(50).then(function () {
        var on = devRows().filter(function (b) { return b.getAttribute('aria-pressed') === 'true'; }).length;
        check('two devices pick together', on === 2, on + ' on');
        clear('Devices').click();
        return wait(50);
      }).then(function () {
        check('and Clear shows every device again', devRows().every(function (b) { return b.getAttribute('aria-pressed') === 'false'; }));
      });
    })
    .then(function () {
      chip('Your turn').click();
      return wait(50);
    })
    .then(function () {
      check('a status chip narrows the view', chip('Your turn').getAttribute('aria-pressed') === 'true');
      document.querySelector('.ca-side-save').click();
      return wait(50);
    })
    .then(function () {
      var input = document.querySelector('.ca-side-name');
      check('saving offers a name for the view', !!input && input.value.indexOf('Your turn') >= 0, input && input.value);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, 'My turns');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      return wait(50);
    })
    .then(function () {
      var v = nav('My turns');
      check('the saved view is listed, as the one showing', !!v && v.classList.contains('is-active'));
      check('and stored', saved().length === 1 && saved()[0].filter.join() === 'unread' && saved()[0].view === 'sessions', JSON.stringify(saved()));
      check('Save this view goes while a saved view shows', !document.querySelector('.ca-side-save'));
      chip('Your turn').click();
      return wait(50);
    })
    .then(function () {
      check('changing a filter leaves the saved view', !nav('My turns').classList.contains('is-active') && !!document.querySelector('.ca-side-save'));
      nav('My turns').click();
      return wait(50);
    })
    .then(function () {
      check('picking it brings its filter back', chip('Your turn').getAttribute('aria-pressed') === 'true');
      expected = 1 + (ageOn() ? 1 : 0);
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', code: 'KeyS', metaKey: true, ctrlKey: true, bubbles: true }));
      return wait(50);
    })
    .then(function () {
      check('⌃⌘S folds the sidebar to a rail', !!document.querySelector('.ca-rail') && !document.querySelector('.ca-side'));
      check('and it stays folded', localStorage.getItem('codealong.sidebarCollapsed') === '1');
      var f = document.querySelector('.ca-rail-btn[aria-label^="Filtered"]');
      var badge = f && f.querySelector('.ca-rail-badge');
      check('the rail counts the filters on', !!badge && badge.textContent === String(expected), (badge && badge.textContent) + ' of ' + expected);
      check('and names them', !!f && f.getAttribute('aria-label').indexOf('Your turn') >= 0, f && f.getAttribute('aria-label'));
      f.click();
      return wait(50);
    })
    .then(function () {
      check('its filter button opens the sidebar', !!document.querySelector('.ca-side') && localStorage.getItem('codealong.sidebarCollapsed') === '0');
      chip('Your turn').click();
      return wait(50);
    })
    .then(function () {
      if (ageOn()) Array.prototype.filter.call(document.querySelectorAll('.ca-side-ages button'), function (b) { return b.textContent === 'All'; })[0].click();
      document.querySelector('.ca-sidetoggle').click();
      return wait(50);
    })
    .then(function () {
      check('with nothing narrowing the view, no badge', !document.querySelector('.ca-rail-btn[aria-label^="Filters"] .ca-rail-badge'));
      document.querySelector('.ca-sidetoggle').click();
      return wait(50);
    })
    .then(function () {
      rightClick(nav('My turns'));
      return wait(100);
    })
    .then(function () {
      var del = window.__menu().filter(function (m) { return m[0] === 'Delete'; })[0];
      check('a saved view’s menu deletes it', !!del, window.__menu().map(function (m) { return m[0]; }).join(' | '));
      if (del) del[1]();
      return wait(50);
    })
    .then(function () {
      check('and it’s gone', !nav('My turns') && saved().length === 0, JSON.stringify(saved()));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
