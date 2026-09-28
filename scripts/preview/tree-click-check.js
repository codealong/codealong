// Checks the sidebar tree's sessions (WorktreeTree.tsx, App.tsx `showSession`): a click shows the
// session in Sessions, selected with its panel and the cursor in its box, without opening it
// anywhere; a double-click opens it where it lives, once.
//   scripts/preview.sh worktrees '' scripts/preview/tree-click-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target'; }); };
  var fire = function (el, type, detail) { el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, detail: detail })); };

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'worktrees');

  var session;
  until(function () { return document.querySelector('.ca-tree-node'); }, 20000)
    .then(function () {
      // Open a worktree that has sessions if none is open yet.
      if (!document.querySelector('.ca-tree-session')) {
        var t = document.querySelector('.ca-tree-toggle:not([disabled])');
        if (t) fire(t, 'click', 1);
      }
      return until(function () { return document.querySelector('.ca-tree-session'); }, 5000);
    })
    .then(function () {
      session = document.querySelector('.ca-tree-session');
      var title = session.querySelector('.ca-tree-title').textContent;
      var before = opens().length;
      fire(session, 'mousedown', 1);
      fire(session, 'mouseup', 1);
      fire(session, 'click', 1);
      return until(function () { return document.querySelector('.ca-sessinsp'); }, 5000)
        .then(function () { return until(function () { return document.activeElement && document.activeElement.closest('.ca-sessinsp .ca-composer'); }, 3000).catch(function () {}); })
        .then(function () {
          var panel = document.querySelector('.ca-sessinsp');
          check('click shows its panel', !!panel && panel.getAttribute('aria-label') === title + ' details', panel && panel.getAttribute('aria-label'));
          var nav = document.querySelector('.ca-nav-item.is-active, [aria-current="page"]');
          check('in Sessions', localStorage.getItem('codealong.view') === 'sessions', localStorage.getItem('codealong.view'));
          check('click opens nothing', opens().length === before, String(opens().length - before));
          var ae = document.activeElement;
          check('cursor in its box', !!(ae && ae.closest('.ca-sessinsp .ca-composer')), ae && ae.tagName);
          // Now a double-click, as the browser sends one: two clicks, then dblclick.
          var el = Array.prototype.find.call(document.querySelectorAll('.ca-tree-session'), function (x) { return x.querySelector('.ca-tree-title').textContent === title; });
          fire(el, 'click', 1);
          fire(el, 'click', 2);
          fire(el, 'dblclick', 2);
          return until(function () { return opens().length > before; }, 3000).catch(function () {}).then(function () {
            var o = opens();
            check('double-click opens it once', o.length === before + 1, String(o.length - before));
            check('where it lives', !!o[o.length - 1] && /^s\|/.test(o[o.length - 1][1].targetId), o[o.length - 1] && o[o.length - 1][1].targetId);
          });
        });
    })
    .catch(function (e) { out.push('error: ' + e.message); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
