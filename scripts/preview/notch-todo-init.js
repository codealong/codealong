// Opens the bar docked in the notch on a todo that's due, for a screenshot of its card
// (TodoCard.tsx, NotchTodo.tsx): the sample todo linked to a worktree is said to be due today, the
// feed lists it as due (barfeed.rs `todo_lists`) and the rest as ready, and the notch opens on it
// from its pill.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_SIZE=1512,860 PREVIEW_INIT=scripts/preview/notch-todo-init.js scripts/preview.sh notch
//   PREVIEW_SCHEME=dark PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_SIZE=1512,860 PREVIEW_INIT=scripts/preview/notch-todo-init.js scripts/preview.sh notch
(function () {
  var d = new Date();
  var today = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  var midnight = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  var DUE = 't-sample4';
  // The sample todos as the fixture has them: the bar's feed comes before any board does.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './todos-fixture.json', false);
  xhr.send();
  var todos = JSON.parse(xhr.responseText).todos;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd === 'get_board') {
      return p.then(function (b) {
        b.todos = b.todos.map(function (t) { return t.id === DUE ? Object.assign({}, t, { due: today }) : t; });
        return b;
      });
    }
    if (cmd === 'bar_feed') {
      return p.then(function (f) {
        var list = todos;
        var bar = function (t, i) { return { id: t.id, title: t.title, project: t.project || '', callsign: null, due: t.id === DUE ? today : t.due || null, since: t.id === DUE ? midnight : t.createdAt || 0, order: i }; };
        f.due = list.filter(function (t) { return t.id === DUE; }).map(bar);
        f.todos = list.map(bar).filter(function (t, i) { return t.id !== DUE && list[i].status !== 'done' && list[i].status !== 'in-progress'; });
        return f;
      });
    }
    return p;
  };
  var opened = false;
  var t = setInterval(function () {
    var shape = document.querySelector('.ca-notch-shape');
    if (!shape || !document.querySelector('.ca-notch-count')) return;
    if (!opened) {
      opened = true;
      shape.click();
      return;
    }
    var pill = document.querySelector('.ca-notch-body .ca-hud-pill.is-due');
    if (!pill) return;
    clearInterval(t);
    pill.click();
  }, 60);
})();
