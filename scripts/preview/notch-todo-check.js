// Checks the todos that wait on you in the bar docked in the notch (NotchTodo.tsx, TodoCard.tsx,
// bar.ts `notchRows`): a todo due counts beside the camera with a calendar and has its pill, and
// the ready ones a pill of their own that the closed count leaves out; the due count opens the
// notch on the todo's card, with the strip's todo actions and keys, the todo as its panel shows
// it and the box at its foot, the strip counting the ready todos apart from what waits; ⌘⏎ in the
// box starts a session on it in its worktree, told the todo, linked to it, and the notch goes on
// to the ready todo; and ⇧⌘E marks that one done.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_SIZE=1512,980 PREVIEW_WAIT=90000 scripts/preview.sh notch '' scripts/preview/notch-todo-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var cardTitle = function () { var t = document.querySelector('.ca-notch-session .ca-todocard .ca-todoinsp-title'); return t ? t.value.trim() : ''; };
  var shows = function (todo) { return function () { return is('session') && !!document.querySelector('.ca-notch-session .ca-todocard') && cardTitle().indexOf(todo.title.slice(0, 20)) >= 0; }; };

  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  // The sample todo linked to a worktree is due today; another, linked to none, is ready.
  var sample = json('./todos-fixture.json').todos;
  var due = sample.filter(function (t) { return t.id === 't-sample4'; })[0];
  var ready = sample.filter(function (t) { return t.id === 't-sample3'; })[0];
  var d = new Date();
  var today = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  var bar = function (t, since, order, day) { return { id: t.id, title: t.title, project: t.project || '', callsign: null, due: day || null, since: since, order: order }; };
  var f = json('./bar-feed.json');
  var feed = Object.assign({}, f, { loading: false, items: [], recent: [], snoozed: [], reviews: [], due: [bar(due, new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime(), 4, today)], todos: [bar(ready, 1, 3)] });
  // The worktree the due todo is linked to, as the board has it: the fixture names it `$lane:2`.
  var linked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.todos = b.todos.map(function (t) { return t.id === due.id ? Object.assign({}, t, { due: today }) : t; });
      linked = (b.todos.filter(function (t) { return t.id === due.id; })[0] || { worktrees: [] }).worktrees[0] || null;
      return b;
    });
  };

  say('notch todo check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () {
      check('has a due todo and a ready one to try', !!due && !!ready && (due.worktrees || []).length > 0);
      window.__emit('bar-feed', feed);
      return until(function () { return document.querySelector('.ca-notch-count.is-due'); }, 3000, 'the due count beside the camera');
    })
    .then(function (count) {
      check('the closed notch counts the todo due, with its calendar, and not the ready one', /1/.test(count.textContent) && !!count.querySelector('.ca-glyph-due') && !document.querySelector('.ca-notch-count.is-ready'), count.outerHTML.slice(0, 160));
      enter();
      return until(function () { return is('pills') && document.querySelector('.ca-notch-body .ca-hud-pill.is-due'); }, 3000, 'its pills');
    })
    .then(function () { return wait(400); })
    .then(function () {
      var duePill = document.querySelector('.ca-notch-body .ca-hud-pill.is-due');
      var readyPill = document.querySelector('.ca-notch-body .ca-hud-pill.is-todos');
      check('the todos due have their pill, and the ready ones theirs', /1 due/.test(duePill.textContent) && !!readyPill && /1 ready/.test(readyPill.textContent), duePill.textContent + ' / ' + (readyPill && readyPill.textContent));
      document.querySelector('.ca-notch-count.is-due').click();
      return until(shows(due), 5000, 'the due todo’s card');
    })
    .then(function () { return wait(800); })
    .then(function () {
      check('the due count opens the notch on the todo’s card', is('session') && is('open'));
      check('its pill is marked', document.querySelector('.ca-notch-body .ca-hud-pill.is-due').classList.contains('is-peeked'));
      var labels = Array.prototype.map.call(document.querySelectorAll('.ca-notch-session .ca-hotkeys .ca-hotkey:not(.is-more)'), function (b) { return b.textContent; });
      check('the strip has the todo’s actions with their keys', labels.join('|') === 'Start⌘⏎|Done⇧⌘E|Snooze⌃⌘1–7|Due…|Back⌃-|Next⌘]|esc', labels.join('|'));
      var note = (q('.ca-notch-session .ca-reply-queue-note') || {}).textContent || '';
      check('the strip says the ready todo is next, apart from what waits', /^1 ready todo · next: /.test(note), note);
      check('the card has the todo as its panel shows it', !!document.querySelector('.ca-notch-session .ca-todocard .ca-todoinsp'));
      var box = q('.ca-notch-session .ca-todocard-box textarea');
      check('and a box at its foot to start it from', !!box);
      type(box, 'Keep it short.');
      box.focus();
      return wait(200).then(function () {
        key({ key: 'Enter', metaKey: true });
        return until(function () { return calls('start_session').length > 0; }, 3000, 'the start');
      });
    })
    .then(function () {
      var spec = last('start_session').spec;
      check('⌘⏎ starts a session on it in its worktree, linked to it', !!linked && spec.laneId === linked && spec.todos.join() === due.id && !!spec.account && !!spec.place, JSON.stringify({ lane: spec.laneId, todos: spec.todos, account: spec.account, place: spec.place }));
      check('told the todo, then what was written', spec.prompt.indexOf(due.title) >= 0 && /Keep it short\.$/.test(spec.prompt), spec.prompt.slice(-80));
      return until(shows(ready), 4000, 'on to the ready todo');
    })
    .then(function () {
      check('and the notch goes on to the ready todo', true);
      check('one linked to no worktree says Start asks where', !!q('.ca-notch-session .ca-todocard-nowhere') && !q('.ca-notch-session .ca-todocard-box'));
      var before = calls('todo_op').length;
      key({ key: 'E', code: 'KeyE', metaKey: true, shiftKey: true });
      return until(function () { return calls('todo_op').length > before; }, 3000, '⇧⌘E').then(function () {
        var op = last('todo_op').op;
        check('⇧⌘E marks it done', op.op === 'update' && op.id === ready.id && op.patch.status === 'done', JSON.stringify(op));
      });
    })
    .then(done)
    .catch(stopped);
})();
