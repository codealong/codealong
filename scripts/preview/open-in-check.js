// Checks the Open in menus (editors.ts, App.tsx): a worktree's right-click menu has Open in
// <your editor> and an Open in submenu of the other editors installed, a session's has Open
// folder in with every one, yours first, and the worktree's Open button lists them too, each
// with its app's icon. Picking one opens the folder there (`l|editor|<id>`). Needs two or more editors installed, and a
// PREVIEW_BIN built with editors.rs if the shared build predates it.
//   PREVIEW_BIN=<engine> scripts/preview.sh worktrees '' scripts/preview/open-in-check.js
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
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target'; }).map(function (c) { return c[1].targetId; }); };
  var right = function (el) { el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); };
  var labels = function () { return window.__menu().map(function (m) { return m[0]; }); };
  var lastItems = function () { var m = window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; }).pop(); return m ? m[1].options.items : []; };
  var pick = function (label) { var m = window.__menu().filter(function (x) { return x[0] === label; })[0]; if (m) m[1](); return !!m; };
  var board;

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'worktrees');

  say('open-in check: waiting for the tree');
  fetch('board.json')
    .then(function (r) { return r.json(); })
    .then(function (b) {
      board = b.apps;
      check('the board lists the editors installed here', (board.editors || []).length >= 2, JSON.stringify(board.editors));
      // The icons come a moment after the board: a menu shown before has none yet.
      return until(function () { return document.querySelector('.ca-tree-node') && window.__calls.some(function (c) { return c[0] === 'editor_icons'; }); }, 20000).then(function () { return wait(500); });
    })
    .then(function () {
      var others = board.editors.filter(function (e) { return e.id !== board.editorId; });
      right(document.querySelector('.ca-tree-node'));
      return wait(200).then(function () {
        var l = labels();
        check('a worktree opens in your editor', l.indexOf('Open in ' + board.editor) >= 0, l.slice(0, 6).join(' | '));
        var sub = l.filter(function (x) { return x.indexOf('Open in › ') === 0; });
        check('and in the others from Open in', sub.join('|') === others.map(function (e) { return 'Open in › ' + e.name; }).join('|'), sub.join(' | '));
        var sub0 = lastItems().filter(function (i) { return i.text === 'Open in'; })[0];
        var icon = sub0 && sub0.items[0].icon;
        check('each with its app icon', icon && icon.width === 36 && icon.rgba.length === 36 * 36 * 4, icon ? icon.width + ' ' + icon.rgba.length : 'none');
        pick('Open in › ' + others[0].name);
        return wait(100).then(function () {
          check('picking one opens the folder there', opens().pop() === 'l|editor|' + others[0].id, opens().join());
        });
      });
    })
    .then(function () {
      var toggle = document.querySelector('.ca-tree-toggle:not([disabled])');
      if (!document.querySelector('.ca-tree-session') && toggle) toggle.click();
      return until(function () { return document.querySelector('.ca-tree-session'); }, 5000);
    })
    .then(function () {
      right(document.querySelector('.ca-tree-session'));
      return wait(200).then(function () {
        var l = labels();
        var sub = l.filter(function (x) { return x.indexOf('Open folder in › ') === 0; });
        var all = [board.editor].concat(board.editors.filter(function (e) { return e.id !== board.editorId; }).map(function (e) { return e.name; }));
        check('a session opens its folder in any editor, yours first', sub.join('|') === all.map(function (n) { return 'Open folder in › ' + n; }).join('|'), sub.join(' | '));
        check('after the session’s own open', l.indexOf(sub[0]) === 1, l.slice(0, 3).join(' | '));
        pick(sub[sub.length - 1]);
        return wait(100).then(function () {
          var last = board.editors.filter(function (e) { return e.name === all[all.length - 1]; })[0];
          check('it opens there', opens().pop() === 'l|editor|' + (last && last.id), opens().join());
        });
      });
    })
    .then(function () {
      // The worktree's panel: its Open button's menu has them after Open in <yours>.
      var node = document.querySelector('.ca-tree-node');
      node.click();
      return until(function () { return document.querySelector('.ca-insp .ca-split-caret'); }, 5000).then(function () {
        document.querySelector('.ca-insp .ca-split-caret').click();
        return wait(100).then(function () {
          var items = Array.prototype.map.call(document.querySelectorAll('.ca-openin .ca-menu-item .ca-menu-label > span:first-child'), function (e) { return e.textContent; });
          var at = items.indexOf('Open in ' + board.editor);
          var next = items.slice(at + 1, at + board.editors.length);
          check('the Open button lists the other editors after yours', at >= 0 && next.every(function (x) { return x.indexOf('Open in ') === 0; }) && next.length === board.editors.length - 1, items.join(' | '));
          var imgs = document.querySelectorAll('.ca-openin .ca-menu-item img.ca-appicon');
          check('with their app icons', imgs.length === board.editors.length && Array.prototype.every.call(imgs, function (i) { return i.src.indexOf('data:image/png') === 0; }), imgs.length + ' icons');
        });
      });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(function () { say(out.join('; ')); });
})();
