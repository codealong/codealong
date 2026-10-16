// Checks the Kanban view (KanbanView.tsx): its four columns, Done folded to a rail that opens it,
// a card's press selecting it with its panel beside the board (a todo's, or a session's), the
// arrows going up a column and across, R marking a todo ready for review, and dragging: a todo
// to In review marks it so, a session to Done marks it done, and a session dropped on a todo's
// card joins it.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh kanban '' scripts/preview/kanban-check.js
// It needs a session card on the board, one running or waiting on you on no todo; the sample
// todos give it its todos.
(function () {
  var pointer = function (target, type, x, y) {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };
  var mid = function (el) { var r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  /** Presses on `el`, moves onto `to` in steps, and lets go there. */
  var drag = function (el, to) {
    var a = mid(el);
    var b = mid(to);
    pointer(el, 'pointerdown', a.x, a.y);
    [0.25, 0.5, 0.75, 1].forEach(function (f) { pointer(window, 'pointermove', a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f); });
    pointer(window, 'pointerup', b.x, b.y);
  };
  var press = function (el) { el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 })); };
  var column = function (id) { return q('.ca-kb-col.is-' + id); };
  var cards = function (id) { var c = column(id); return c ? all('.ca-kb-cardrow', c) : []; };
  var face = function (row) { var f = q('.ca-cv-card', row); return f ? f.dataset.id : ''; };
  var todoOps = function () { return calls('todo_op').map(function (c) { return c[1].op; }); };
  var since = 0;
  var opsSince = function () { return todoOps().slice(since); };

  until(function () { return column('todo') && q('.ca-kb-rail'); }, 30000, 'the Kanban view’s columns')
    .then(function () {
      var heads = all('.ca-kb-col .ca-kb-head span:nth-child(2)').map(function (s) { return s.textContent; });
      check('it has To do, In progress and In review, Done folded to a rail', heads.join('|') === 'To do|In progress|In review' && !!q('.ca-kb-rail[data-kb-column="done"]'), heads.join('|'));
      check('To do holds only todos, in the box’s column', cards('todo').length > 0 && cards('todo').every(function (r) { return face(r).indexOf('todo:') === 0; }) && !!q('.ca-kb-col.is-todo .ca-todonew'), cards('todo').map(face).join(' '));
      q('.ca-kb-rail').click();
      return until(function () { return column('done'); }, 5000, 'Done opened');
    })
    .then(function () {
      check('the rail opens Done as a column', !!column('done') && !q('.ca-kb-rail'));
      q('.ca-kb-col.is-done .ca-kb-fold').click();
      return until(function () { return q('.ca-kb-rail'); }, 5000, 'Done folded again');
    })
    .then(function () {
      // A todo's press selects it, with its panel beside the board.
      var first = cards('todo')[0];
      press(first);
      return until(function () { return first.classList.contains('is-selected') && q('.ca-todoinsp'); }, 5000, 'the todo’s panel');
    })
    .then(function () {
      check('a todo’s press selects it and shows its panel', true);
      // The arrows: across to the next column that has cards, and back.
      key(document.body, { key: 'ArrowRight' });
      return until(function () { var s = q('.ca-kb-cardrow.is-selected'); return s && !q('.ca-kb-col.is-todo .ca-kb-cardrow.is-selected') && s; }, 5000, '→ moving on').then(function (to) {
        check('→ goes to the next column’s card', true, face(to));
        key(document.body, { key: 'ArrowLeft' });
        return until(function () { return q('.ca-kb-col.is-todo .ca-kb-cardrow.is-selected'); }, 5000, '← back to To do');
      });
    })
    .then(function () {
      check('← comes back to To do', true);
      // R marks it ready for review.
      since = todoOps().length;
      key(document.body, { key: 'r' });
      return until(function () { return opsSince().length > 0; }, 5000, 'R’s change');
    })
    .then(function () {
      var op = opsSince()[0];
      check('R marks the todo ready for review', op && op.op === 'update' && op.patch && op.patch.review && op.patch.review.since === 0, JSON.stringify(op));
      var session = all('.ca-kb-cardrow').filter(function (r) { return face(r).indexOf('session:') === 0; })[0];
      if (!session) throw new Error('no session card on the board: one running or waiting on you, on no todo');
      press(session);
      return until(function () { return session.classList.contains('is-selected') && q('.ca-sessinsp'); }, 5000, 'the session’s panel').then(function () { return session; });
    })
    .then(function (session) {
      check('a session’s press shows its panel, with its conversation', true);
      // A todo dragged from To do to In review is marked so.
      since = todoOps().length;
      var todo = cards('todo')[cards('todo').length - 1];
      drag(todo, column('review'));
      return until(function () { return opsSince().length > 0; }, 5000, 'the drop on In review')
        .then(function () {
          var op = opsSince()[0];
          check('a todo dropped on In review is marked ready for review', op && op.op === 'update' && op.patch.review, JSON.stringify(op));
          // A session dropped on Done is marked done, as its menu marks it: one that isn't
          // working, which Mark as done would ask about first.
          var idle = all('.ca-kb-cardrow').filter(function (r) { return face(r).indexOf('session:') === 0 && !q('.ca-cv-card.st-working', r); })[0];
          if (!idle) return note('no session card that isn’t working, to drop on Done');
          var before = calls('session_done').length;
          drag(idle, q('.ca-kb-rail'));
          return until(function () { return calls('session_done').length > before; }, 5000, 'session_done').then(function () {
            check('a session dropped on Done is marked done', calls('session_done').slice(-1)[0][1].key === face(idle).slice('session:'.length));
          });
        })
        .then(function () {
          // A session dropped on a todo's card joins it, and leaves the board for that card.
          var other = all('.ca-kb-cardrow').filter(function (r) { return face(r).indexOf('session:') === 0 && r.isConnected; })[0];
          if (!other) return note('no session card left to drop on a todo');
          since = todoOps().length;
          drag(other, cards('todo')[0]);
          return until(function () { return opsSince().length > 0; }, 5000, 'the drop on a todo').then(function () {
            var op = opsSince()[0];
            check('a session dropped on a todo’s card joins it', op && op.op === 'link' && op.linked && op.session === face(other).slice('session:'.length), JSON.stringify(op));
          });
        });
    })
    .then(done)
    .catch(stopped);
})();
