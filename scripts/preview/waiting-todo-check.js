// Checks the todos due in the waiting window (Waiting.tsx, quick.ts `windowRows`): with due todos
// put first in your order (`[queue] order`), the window opens on the one most overdue, as its card
// (TodoCard.tsx) with the strip a todo has (TodoStrip.tsx) as the window's title bar, saying the
// next; ⇧⌘E marks it done and goes on to the next due, and ⌃⌘1 snoozes that one and goes on.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-todo-check.js
//   PREVIEW_PLATFORM=linux PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-todo-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.queue = { order: ['due', 'needs', 'reviews', 'unread', 'ready'], due_within: 0 };
      // Nothing moved to the top, which would go ahead of every kind.
      board.lanes.forEach(function (l) { l.sessions.forEach(function (x) { delete x.raised; }); });
      return board;
    });
  };
  // ⌘ is Ctrl on Linux and Windows, and ⌃⌘ Ctrl+Alt (platform.ts).
  var mac = !window.__CODEALONG_PLATFORM__;
  var cmd = mac ? { metaKey: true } : { ctrlKey: true };
  var ctrlCmd = mac ? { metaKey: true, ctrlKey: true } : { ctrlKey: true, altKey: true };
  var press = function (mods, init) {
    if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, mods, init)));
  };
  var strip = function () { return q('.ca-waiting .ca-todocard > .ca-reply-queue'); };
  var title = function () { var t = q('.ca-waiting .ca-todocard .ca-todoinsp-title'); return t ? t.value : ''; };
  var words = function (s) { return all('.ca-hotkeys > .ca-hotkey:not(.is-more)', s).map(function (b) { var w = b.querySelector(':scope > span:not([class])'); return w ? w.textContent.trim() : ''; }).filter(Boolean).join('|'); };
  var said = function (s) { var n = q('.ca-reply-queue-note', s); return n ? n.textContent : ''; };
  var op = function (id) { return sent('todo_op').filter(function (a) { return a.op.id === id; }).pop(); };

  say('waiting todo check: waiting for the window');
  until(function () { return strip() && /Keep the sidebar/.test(title()); }, 20000, 'the waiting window on the todo most overdue')
    .then(function () {
      var s = strip();
      check('it opens on the todo most overdue, as a todo’s card', !!q('.ca-waiting > .ca-todocard'));
      check('its strip has the todo’s actions, back and next', words(s) === 'Start|Done|Snooze|Due…|Back|Next', words(s));
      check('the strip is the window’s title bar', s.classList.contains('is-titlebar') && s.getAttribute('data-tauri-drag-region') === 'deep');
      check('it says the todo due next', /next: Show how long/.test(said(s)), said(s));
      check('its box starts a session on it', !!q('.ca-waiting .ca-todocard-box textarea') || !!q('.ca-waiting .ca-todocard-nowhere'));
      press(cmd, { key: 'E', code: 'KeyE', shiftKey: true });
      return until(function () { return /Show how long/.test(title()) && title(); }, 3000, '⇧⌘E going on to the next todo due');
    })
    .then(function () {
      var done = op('t-sample1');
      check('⇧⌘E marks it done', !!done && done.op.patch.status === 'done', JSON.stringify(done));
      press(ctrlCmd, { key: '1', code: 'Digit1' });
      return until(function () { return op('t-sample2') && !/Show how long/.test(title()); }, 3000, '⌃⌘1 snoozing it and going on');
    })
    .then(function () {
      var snoozed = op('t-sample2');
      check('⌃⌘1 snoozes it, its due date as it is, and goes on', !!snoozed && typeof snoozed.op.patch.snoozed === 'number' && snoozed.op.patch.snoozed > Date.now() && !('due' in snoozed.op.patch), JSON.stringify(snoozed));
      done();
    })
    .catch(stopped);
})();
