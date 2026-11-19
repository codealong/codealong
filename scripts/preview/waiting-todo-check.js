// Checks the todos due in the waiting window (Waiting.tsx, quick.ts `windowRows`), with only four
// sample todos waiting, due ones put first in your order (`[queue] order`): the window opens on the
// one most overdue, as its card (TodoCard.tsx) with the strip a todo has (TodoStrip.tsx) as the
// window's title bar, saying the next; a new due date from Due… goes on to the next due; one marked
// done in another window while it shows stays, what's written in its box with it, until the
// engine's board has it done, and then the window goes on; ⌃⌘1 snoozes the next and goes on; and
// the last given another day leaves the window saying nothing waits, which it still says once the
// todo would have been read again. (⇧⌘E marking one done is the notch's check: the card and its
// strip are the same.)
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-todo-check.js
//   PREVIEW_PLATFORM=linux PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-todo-check.js
(function () {
  var d = new Date();
  var today = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  var midnight = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.queue = { order: ['due', 'needs', 'reviews', 'unread', 'ready'], due_within: 0 };
      // Nothing else waits: no session, no review.
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (x) {
          delete x.raised;
          if (x.status === 'needs' || x.status === 'error' || x.status === 'unread') x.status = 'idle';
        });
      });
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      // The two sample todos due, and two more due today after them (the stub's stand-in for the
      // engine's list, fake-tauri.js `withFixture`).
      // The one whose box the check writes in starts in a worktree of its own here, whatever
      // sessions this Mac has for the fixture's to stand for.
      var here = board.lanes.filter(function (l) { return !l.hidden && !l.device; })[0];
      board.todos.forEach(function (t) { if (t.id === 't-sample2' && here) t.worktrees = [here.id]; });
      ['t-sample3', 't-sample4'].forEach(function (id) {
        var i = board.todos.findIndex(function (t) { return t.id === id; });
        var t = board.todos[i];
        t.due = today;
        board.todoQueue.ready = board.todoQueue.ready.filter(function (x) { return x.id !== id; });
        if (!board.todoQueue.due.some(function (x) { return x.id === id; })) board.todoQueue.due.push({ id: id, title: t.title, project: t.project || '', callsign: null, due: today, since: midnight, order: i });
      });
      return board;
    });
  };
  // ⌃⌘ is Ctrl+Alt on Linux and Windows (platform.ts).
  var mac = !window.__CODEALONG_PLATFORM__;
  var ctrlCmd = mac ? { metaKey: true, ctrlKey: true } : { ctrlKey: true, altKey: true };
  var press = function (mods, init) {
    if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, mods, init)));
  };
  var strip = function () { return q('.ca-waiting .ca-todocard > .ca-reply-queue'); };
  var title = function () { var t = q('.ca-waiting .ca-todocard .ca-todoinsp-title'); return t ? t.value : ''; };
  var shows = function (start) { return function () { return title().indexOf(start) === 0 && title(); }; };
  var words = function (s) { return all('.ca-hotkeys > .ca-hotkey:not(.is-more)', s).map(function (b) { var w = b.querySelector(':scope > span:not([class])'); return w ? w.textContent.trim() : ''; }).filter(Boolean).join('|'); };
  var said = function (s) { var n = q('.ca-reply-queue-note', s); return n ? n.textContent : ''; };
  var op = function (id) { return sent('todo_op').filter(function (a) { return a.op.id === id; }).pop(); };
  var end = function () { return q('.ca-waiting .ca-queue-end'); };
  var dueTomorrow = function () {
    var due = all('.ca-hotkey', strip()).filter(function (b) { var w = b.querySelector(':scope > span'); return w && w.textContent === 'Due…'; })[0];
    var before = menus();
    due.click();
    return until(function () { return menus() > before; }, 3000, 'the Due… menu').then(function () {
      var tomorrow = menuItem('Tomorrow');
      if (!tomorrow) throw new Error('Due… has no Tomorrow: ' + menuLabels().join(' | '));
      tomorrow();
    });
  };
  var given = function (id) { var a = op(id); return !!a && /^\d{4}-\d\d-\d\d$/.test(a.op.patch.due) && a.op.patch.due !== today && Object.keys(a.op.patch).length === 1; };

  say('waiting todo check: waiting for the window');
  until(function () { return strip() && /Keep the sidebar/.test(title()); }, 20000, 'the waiting window on the todo most overdue')
    .then(function () {
      var s = strip();
      check('it opens on the todo most overdue, as a todo’s card', !!q('.ca-waiting > .ca-todocard'));
      check('its strip has the todo’s actions, back and next', words(s) === 'Start|Done|Snooze|Due…|Back|Next', words(s));
      check('the strip is the window’s title bar', s.classList.contains('is-titlebar') && s.getAttribute('data-tauri-drag-region') === 'deep');
      check('it says the todos due after it', /^3 more waiting · next: Show how long/.test(said(s)), said(s));
      check('its box starts a session on it', !!q('.ca-waiting .ca-todocard-box textarea') || !!q('.ca-waiting .ca-todocard-nowhere'));
      return dueTomorrow().then(function () { return until(shows('Show how long'), 3000, 'a new due date going on to the next todo due'); });
    })
    .then(function () {
      check('a new due date from Due… goes on to the next todo due', given('t-sample1'), JSON.stringify(op('t-sample1')));
      // Something is written in its box, and another window marks it done (ahead.ts
      // `todoLeftAhead`), as the main window's strip does, ahead of the engine. (The preview calls
      // every page `main`, so the other is named otherwise.)
      type(q('.ca-waiting .ca-todocard-box textarea'), 'Start with the checks page');
      window.__emit('ahead', { from: 'notch', put: { id: 'notch:check:1', change: { kind: 'todo-left', ids: ['t-sample2'] }, at: Date.now(), doneAt: null, shownAt: null } });
      return wait(800);
    })
    .then(function () {
      var box = q('.ca-waiting .ca-todocard-box textarea');
      check('another window’s change ahead of the engine keeps the card, and what’s written in it', /Show how long/.test(title()) && !!box && box.value === 'Start with the checks page', title() + ' / ' + (box && box.value));
      // The engine's board has it done: it no longer waits.
      return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) {
        b.todoQueue = { due: b.todoQueue.due.filter(function (t) { return t.id !== 't-sample2'; }), ready: b.todoQueue.ready };
        b.todos = b.todos.map(function (t) { return t.id === 't-sample2' ? Object.assign({}, t, { status: 'done' }) : t; });
        b.generatedAt = Date.now();
        window.__emit('board', b);
        return until(shows('Cut the board'), 3000, 'the window going on once the board has it done');
      });
    })
    .then(function () {
      check('once the engine’s board has it done, the window goes on', !op('t-sample2'));
      press(ctrlCmd, { key: '1', code: 'Digit1' });
      return until(shows('Write up the config'), 3000, '⌃⌘1 snoozing it and going on');
    })
    .then(function () {
      var snoozed = op('t-sample3');
      check('⌃⌘1 snoozes it, its due date as it is, and goes on', !!snoozed && typeof snoozed.op.patch.snoozed === 'number' && snoozed.op.patch.snoozed > Date.now() && !('due' in snoozed.op.patch), JSON.stringify(snoozed));
      return dueTomorrow().then(function () { return until(function () { return !!end() && !q('.ca-waiting .ca-todocard'); }, 3000, 'the end of what waits'); });
    })
    .then(function () {
      check('the last given another day leaves the window saying nothing else waits', given('t-sample4') && /Nothing else waits on you/.test(end().textContent), end().textContent.slice(0, 80));
      // The board hasn't caught up: the todo is still due on it, and mustn't come back meanwhile.
      return wait(1500);
    })
    .then(function () {
      check('and it isn’t brought back before the board has its new day', !!end() && !q('.ca-waiting .ca-todocard'), title());
      done();
    })
    .catch(stopped);
})();
