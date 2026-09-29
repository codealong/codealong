// Checks the sidebar (Sidebar.tsx, App.tsx): a status chip narrows the view, the view saves
// under a name and comes back when picked, ⌃⌘S folds the sidebar to a rail whose filter button
// counts the filters on and opens it again, and a saved view's menu deletes it.
//   scripts/preview.sh sessions '' scripts/preview/sidebar-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var nav = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side .ca-nav'), function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === label; })[0];
  };
  var chip = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-side-chip'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var saved = function () { return JSON.parse(localStorage.getItem('codealong.savedViews') || '[]'); };
  var ageOn = function () { var on = document.querySelector('.ca-side-ages button.is-on'); return on && on.textContent !== 'All'; };
  var expected;

  say('sidebar check: waiting for the sidebar');
  until(function () { return nav('Sessions') && chip('Your turn'); }, 20000)
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
      check('and stored', saved().length === 1 && saved()[0].filter === 'unread' && saved()[0].view === 'sessions', JSON.stringify(saved()));
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
      nav('My turns').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
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
    .then(function () { say('done: ' + out.join('; ')); });
})();
