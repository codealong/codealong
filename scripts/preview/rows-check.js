// Checks a board list across pushes, now that unchanged rows skip rendering (memoRow.ts): rows
// show new data, a todo linked later appears, a double-click opens the pushed lane, the arrow
// keys move the selection, and a hidden window catches up when it's shown.
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/rows-check.js   (or sessions)
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
  var text;
  var check = function (name, ok) { out.push(name + (ok ? ' ok' : ' FAIL')); };
  var hidden = false;
  Object.defineProperty(document, 'hidden', { configurable: true, get: function () { return hidden; } });
  fetch('./board.json').then(function (r) { return r.text(); }).then(function (t) {
    text = t;
    return until(function () { return document.querySelector('.ca-board-list [role="option"][id^="row-"]'); }, 20000);
  }).then(function () {
    var first = document.querySelector('.ca-board-list [role="option"][id^="row-"]');
    var key = first.id.slice(4);
    check('id from key', document.getElementById(first.id) === first && key.indexOf('::') > 0);
    var parts = key.split('::');
    var laneId = parts[0], sessionKey = parts[1];
    // 1. A new title for the row's session shows.
    var b = JSON.parse(text);
    var lane = b.lanes.find(function (l) { return l.id === laneId; });
    var s = lane.sessions.find(function (x) { return x.key === sessionKey; });
    s.title = 'PUSHED TITLE ONE';
    window.__emit('board', b);
    return until(function () { return document.body.textContent.indexOf('PUSHED TITLE ONE') >= 0; }, 3000).then(function () {
      check('title', true);
      // 2. A todo linked to the session afterwards shows on its line.
      var b2 = JSON.parse(JSON.stringify(b));
      var t = JSON.parse(JSON.stringify(b2.todos[0]));
      t.id = 't-check'; t.title = 'LINKED TODO CHECK'; t.status = 'todo'; t.sessions = [sessionKey]; t.worktrees = []; t.project = lane.project;
      b2.todos.push(t);
      window.__emit('board', b2);
      return until(function () { var row = document.getElementById('row-' + key); return row && row.closest('[class]') && document.getElementById('row-' + key).outerHTML.indexOf('LINKED TODO CHECK') >= 0; }, 3000)
        .then(function () { check('todo chip', true); }, function () { check('todo chip', false); })
        .then(function () { return b2; });
    }, function () { check('title', false); return b; });
  }).then(function (b) {
    // 3. A double-click opens with the lane from the latest push.
    var first = document.querySelector('.ca-board-list [role="option"][id^="row-"]');
    var n = window.__calls.length;
    first.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, detail: 2 }));
    return wait(200).then(function () {
      var calls = window.__calls.slice(n).filter(function (c) { return c[0] === 'open_target'; });
      check('open', calls.length === 1 && first.id.slice(4).indexOf(calls[0][1].laneId) === 0);
      // 4. Arrow down moves the selection to another row that exists.
      first.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
      return wait(100);
    }).then(function () {
      var list = document.querySelector('.ca-board-list');
      var before = list.getAttribute('aria-activedescendant');
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
      return wait(200).then(function () {
        var after = list.getAttribute('aria-activedescendant');
        var el = after && document.getElementById(after);
        check('arrow', before && after && before !== after && el && el.getAttribute('aria-selected') === 'true');
        return b;
      });
    });
  }).then(function (b) {
    // 5. A hidden window holds the push and shows it when it's seen again.
    hidden = true;
    var b3 = JSON.parse(JSON.stringify(b));
    var first = document.querySelector('.ca-board-list [role="option"][id^="row-"]');
    var parts = first.id.slice(4).split('::');
    var lane = b3.lanes.find(function (l) { return l.id === parts[0]; });
    lane.callsign = 'SEEN WHEN SHOWN';
    window.__emit('board', b3);
    return wait(400).then(function () {
      check('held while hidden', document.body.textContent.indexOf('SEEN WHEN SHOWN') < 0);
      hidden = false;
      document.dispatchEvent(new Event('visibilitychange'));
      return until(function () { return document.body.textContent.indexOf('SEEN WHEN SHOWN') >= 0; }, 3000).then(function () { check('shown', true); }, function () {
        check('shown', false);
        var where = [];
        document.querySelectorAll('*').forEach(function (el) { if (el.children.length === 0 && el.textContent.indexOf('PUSHED TITLE ONE') >= 0) { var p = el, path = []; for (var i = 0; i < 4 && p; i++, p = p.parentElement) path.push(p.className || p.tagName); where.push(path.join(' < ')); } });
        out.push('old title at: ' + where.join(' | '));
        out.push('row text: ' + first.textContent.slice(0, 120));
        out.push('row key ' + first.id + ' found=' + !!lane.sessions.find(function (x) { return x.key === parts[1]; }));
        window.__emit('board', b3);
        return until(function () { return document.body.textContent.indexOf('SEEN WHEN SHOWN') >= 0; }, 3000).then(function () { check('shown on a later push', true); }, function () { check('shown on a later push', false); });
      });
    });
  }).then(function () { say('done: ' + out.join(', ')); }, function (e) { say('done: error ' + e.message + ' after ' + out.join(', ')); });
})();
