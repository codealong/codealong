// Checks a todo's right-click menu (menus.ts `todoMenu`) where todos are, as far as each window
// can change one: the Todo view has its status, waiting, due date with the panel's picker, labels,
// project and delete; the quick panel's all but the picker and delete (it can't put a todo back);
// a todo's card on the desktop its status and due date. Picking Done and a day sends the change.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/todo-menus-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh quick '' scripts/preview/todo-menus-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_PIN=todo scripts/preview.sh desktop '' scripts/preview/todo-menus-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var until = function (f, ms, what) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { var v = f(); if (v) return res(v); if (Date.now() - s > ms) return rej(new Error('timeout: ' + what)); setTimeout(p, 20); })();
    });
  };
  var where = window.__CODEALONG_DESKTOP__ ? 'desktop' : window.__CODEALONG_QUICK__ ? 'quick' : 'main';
  var target = { desktop: '.ca-desk', quick: '.ca-quick-row.is-todo', main: '.ca-todorow-title' }[where];
  var menus = function () { return window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; }).length; };
  var ops = function () { return window.__calls.filter(function (c) { return c[0] === 'todo_op'; }).map(function (c) { return c[1].op; }); };
  var open = function () {
    var el = document.querySelector(target);
    var before = menus();
    el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    return until(function () { return menus() > before; }, 3000, 'its menu').then(function () { return window.__menu(); });
  };
  var has = function (items, label) { return items.some(function (x) { return x[0].replace(/ ✓$/, '') === label; }); };
  var starts = function (items, prefix) { return items.some(function (x) { return x[0].indexOf(prefix) === 0; }); };
  var pick = function (items, test) {
    var hit = items.find(function (x) { return test(x[0]); });
    if (hit) hit[1]();
    return hit;
  };
  var tomorrow = (function () {
    var d = new Date();
    d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1);
    return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  })();
  say('todo menus check: waiting');
  until(function () { return document.querySelector(target); }, 20000, 'a todo')
    // A moment for the data behind it: a desktop card is there before its feed is.
    .then(function () { return wait(1500); })
    .then(open)
    .then(function (items) {
      var all = items.map(function (x) { return x[0]; }).join(' | ');
      check('it has the statuses', has(items, 'Todo') && has(items, 'In progress') && has(items, 'Done'), all);
      check('one is checked', items.filter(function (x) { return /^(Todo|In progress|Done) ✓$/.test(x[0]); }).length === 1);
      check('and the due dates', has(items, 'Due date › Tomorrow'));
      check('the panel’s picker only in the main window', has(items, 'Due date › Pick a date…') === (where === 'main'));
      var full = where !== 'desktop';
      check('waiting where every todo is known', (has(items, 'Mark as waiting') || starts(items, 'Stop waiting')) === full);
      check('labels and project where they are known', starts(items, 'Labels › ') === full && starts(items, 'Move to project › ') === full);
      check('delete only where it can be undone', has(items, 'Delete todo') === (where === 'main'));
      check('copy as prompt everywhere', has(items, 'Copy as prompt'));
      var n = ops().length;
      pick(items, function (l) { return l === 'Done' || l === 'Done ✓'; });
      return until(function () { return ops().length > n; }, 3000, 'the change').then(function () {
        var op = ops()[ops().length - 1];
        check('Done sends the change', op.op === 'update' && op.patch.status === 'done', JSON.stringify(op));
      });
    })
    .then(function () { return wait(300); })
    .then(open)
    .then(function (items) {
      var n = ops().length;
      pick(items, function (l) { return l.indexOf('Due date › Tomorrow') === 0; });
      return until(function () { return ops().length > n; }, 3000, 'the day').then(function () {
        var op = ops()[ops().length - 1];
        check('a day sends the due date', op.op === 'update' && op.patch.due === tomorrow, JSON.stringify(op));
      });
    })
    .then(function () { say('done: ' + where + ': ' + out.join('; ')); })
    .catch(function (e) { say('done: ' + where + ': ' + out.concat(['FAIL ' + e.message]).join('; ')); });
})();
