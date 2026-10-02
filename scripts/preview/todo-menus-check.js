// Checks a todo's right-click menu (menus.ts `todoMenu`) where todos are, as far as each window
// can change one: the Todo view has its status, waiting, due date with the panel's picker, labels,
// project and delete; the quick panel's all but the picker and delete (it can't put a todo back);
// a todo's card on the desktop its status and due date. Picking Done and a day sends the change.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/todo-menus-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh quick '' scripts/preview/todo-menus-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_PIN=todo scripts/preview.sh desktop '' scripts/preview/todo-menus-check.js
(function () {
  var where = window.__CODEALONG_DESKTOP__ ? 'desktop' : window.__CODEALONG_QUICK__ ? 'quick' : 'main';
  var target = { desktop: '.ca-desk', quick: '.ca-quick-row.is-todo', main: '.ca-todorow-title' }[where];
  var bare = function (l) { return l.replace(/ ✓$/, ''); };
  var has = function (labels, label) { return labels.some(function (l) { return bare(l) === label; }); };
  var starts = function (labels, prefix) { return labels.some(function (l) { return l.indexOf(prefix) === 0; }); };
  var choose = function (labels, test) {
    var label = labels.filter(test)[0];
    if (!label) throw new Error('no item for ' + test);
    menuItem(label)();
  };
  // The change it sends once an item is picked.
  var sends = function (labels, test) {
    var n = sent('todo_op').length;
    choose(labels, test);
    return until(function () { return sent('todo_op').length > n; }, 3000, 'the change').then(function () { return last('todo_op').op; });
  };
  var tomorrow = (function () {
    var d = new Date();
    d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1);
    return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  })();
  say('todo menus check: waiting');
  until(function () { return q(target); }, 20000, 'a todo in ' + where)
    // A moment for the data behind it: a desktop card is there before its feed is.
    .then(function () { return wait(1500); })
    .then(function () { return menuOf(q(target)); })
    .then(function (labels) {
      var all = where + ': ' + labels.join(' | ');
      note(all);
      check('it has the statuses', has(labels, 'Todo') && has(labels, 'In progress') && has(labels, 'Done'), all);
      check('the one it is at is checked', labels.filter(function (l) { return /^(Todo|In progress|Done) ✓$/.test(l); }).length === 1, all);
      check('and the due dates', has(labels, 'Due date › Tomorrow'), all);
      check('the panel’s picker only in the main window', has(labels, 'Due date › Pick a date…') === (where === 'main'), all);
      var full = where !== 'desktop';
      check('waiting where every todo is known', (has(labels, 'Mark as waiting') || starts(labels, 'Stop waiting')) === full, all);
      check('labels and project where they are known', starts(labels, 'Labels › ') === full && starts(labels, 'Move to project › ') === full, all);
      check('delete only where it can be undone', has(labels, 'Delete todo') === (where === 'main'), all);
      check('copy as prompt everywhere', has(labels, 'Copy as prompt'), all);
      return sends(labels, function (l) { return bare(l) === 'Done'; });
    })
    .then(function (op) {
      check('Done sends the change', op.op === 'update' && op.patch.status === 'done', JSON.stringify(op));
      return wait(300).then(function () { return menuOf(q(target)); });
    })
    .then(function (labels) { return sends(labels, function (l) { return bare(l) === 'Due date › Tomorrow'; }); })
    .then(function (op) { check('a day sends the due date', op.op === 'update' && op.patch.due === tomorrow, JSON.stringify(op)); })
    .then(done)
    .catch(stopped);
})();
