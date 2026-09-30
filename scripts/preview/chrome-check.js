// Checks a session's Chrome item (menus.ts, board.rs): the most recent session that drove Chrome
// has Show in Chrome (or Reopen … in Chrome) in its right-click menu, with the profile when
// Chrome has more than one, and picking it opens `s|<key>|chrome`; its panel's Open menu lists it
// apart from the ways to open the session. Needs a session on this Mac that used Claude in
// Chrome, and a PREVIEW_BIN built with chrometabs.rs if the shared build predates it.
//   PREVIEW_BIN=<engine> PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/chrome-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { var v = f(); if (v) return res(v); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 20); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var menus = function () { return window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; }).length; };
  var labels = function () { return window.__menu().map(function (m) { return m[0]; }); };
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target'; }).map(function (c) { return c[1].targetId; }); };
  var lane, s, target;

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  say('chrome check: waiting for the board');
  fetch('board.json')
    .then(function (r) { return r.json(); })
    .then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (x) {
          if (x.chrome && !x.visitor && (!s || x.chrome.at > s.chrome.at)) {
            s = x;
            lane = l;
          }
        });
      });
      if (!s) throw new Error('no session here has used Chrome');
      target = lane.targets.filter(function (t) { return t.id === 's|' + s.key + '|chrome'; })[0];
      check('its worktree lists its Chrome tabs apart', target && target.group === 'browser', target ? target.label + ' · ' + target.group : 'none');
      // Its row, by its key: a fork has the same title.
      return until(function () { return document.querySelector('.ca-bay-lanes [id^="row-"][id$="' + s.key + '"]'); }, 20000);
    })
    .then(function (row) {
      var before = menus();
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return until(function () { return menus() > before; }, 3000).then(function () {
        var l = labels();
        var i = l.indexOf(target.label);
        check('its menu has ' + target.label, i >= 0, l.slice(0, 8).join(' | '));
        check('after the ways to open it', i > 0, l.slice(0, i + 1).join(' | '));
        window.__menu()[i][1]();
        return wait(100).then(function () {
          check('picking it opens its tabs', opens().pop() === 's|' + s.key + '|chrome', opens().join());
          // Rows select on mousedown.
          row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
        });
      });
    })
    .then(function () { return until(function () { return document.querySelector('.ca-sessinsp [aria-label="Other ways to open"]'); }, 5000); })
    .then(function (caret) {
      caret.click();
      return wait(200).then(function () {
        var items = Array.prototype.map.call(document.querySelectorAll('.ca-sessinsp .ca-openin .ca-menu [role="menuitem"]'), function (el) { return el.textContent; });
        check('its panel offers it too', items.some(function (t) { return t.indexOf(target.label) >= 0; }), items.join(' | '));
        say('done: ' + out.join('; '));
      });
    })
    .catch(function (e) { say('done: FAIL ' + e.message + ' after ' + out.join('; ')); });
})();
