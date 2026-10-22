// Checks archiving todos in the Todo view (useTodoView.ts, TodoView.tsx, menus.ts `todoMenu`): the
// Archived fold after Done, folded until Show, the latest archived first; Archive todo in a row's
// menu sends the change, moves the row into the fold and says so with an Undo; A on an archived
// row brings it back out; an archived todo's panel says Archived and offers Unarchive todo.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/todo-archive-check.js
(function () {
  var row = function (title, root) {
    return all('.ca-todorow', root).filter(function (r) { return r.querySelector('.ca-todorow-title').textContent.indexOf(title) === 0; })[0];
  };
  var titles = function (root) { return all('.ca-todorow-title', root).map(function (t) { return t.textContent; }); };
  var fold = function () { return q('.ca-todo-archived'); };
  var count = function () { var c = q('.ca-todo-archived .ca-bay-count'); return c ? c.textContent : 'none'; };
  var select = function (r) { r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 })); };
  var lastOp = function () { var o = last('todo_op'); return JSON.stringify(o && o.op); };
  var shelved = "Try a second color for the bar's mark";

  say('todo archive check: waiting for todos');
  until(function () { return row('Write up the config.toml'); }, 20000)
    .then(function () {
      check('archived todos are under Archived, after Done', !!fold() && fold().previousElementSibling === q('.ca-todo-done'), fold() ? fold().textContent : 'no fold');
      check('which counts them', count() === '2', count());
      check('and is folded', titles(fold()).length === 0, titles(fold()).join(' | '));
      check('they are in no other section', !row(shelved) && !row('Fix the scroll jump'));
      q('.ca-todo-archived .ca-linkbtn').click();
      return until(function () { return titles(fold()).length > 0; }, 3000, 'the archived rows');
    })
    .then(function () {
      check('Show lists them, the latest archived first', titles(fold()).join(' | ') === shelved + ' | Fix the scroll jump in long conversations', titles(fold()).join(' | '));
      select(row(shelved, fold()));
      return until(function () { return q('.ca-todoinsp'); }, 6000, 'its panel');
    })
    .then(function () {
      var tags = all('.ca-todoinsp .ca-insp-title .ca-tag').map(function (t) { return t.textContent; });
      check('its panel says it is archived', tags.indexOf('Archived') >= 0, tags.join(' | '));
      return menuOf(row(shelved, fold()));
    })
    .then(function (labels) {
      check('its menu brings it back', labels.indexOf('Unarchive todo') >= 0, labels.join(' | '));
      // A on the archived row selected brings it back out.
      document.activeElement && document.activeElement.blur && document.activeElement.blur();
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', code: 'KeyA', bubbles: true }));
      return until(function () { return count() === '1'; }, 3000, 'it to leave the fold').catch(function () {});
    })
    .then(function () {
      check('A unarchives it', lastOp() === JSON.stringify({ op: 'update', id: 't-sample9', patch: { archived: false } }), lastOp());
      check('and it is back in Ready', count() === '1' && !!row(shelved) && !row(shelved, fold()), count());
      return menuOf(row('Write up the config.toml'));
    })
    .then(function (labels) {
      check('an open todo’s menu archives it', labels.indexOf('Archive todo') >= 0, labels.join(' | '));
      menuItem('Archive todo')();
      return until(function () { return count() === '2'; }, 3000, 'it in the fold').catch(function () {});
    })
    .then(function () {
      check('Archive todo sends the change', lastOp() === JSON.stringify({ op: 'update', id: 't-sample4', patch: { archived: true } }), lastOp());
      check('and the row goes under Archived', !!row('Write up the config.toml', fold()) && titles(fold())[0].indexOf('Write up the config.toml') === 0, titles(fold()).join(' | '));
      var toast = q('.ca-toast');
      check('the toast says so, with an undo', !!toast && /Archived “Write up the config\.toml/.test(toast.textContent) && /Undo/.test(toast.textContent), toast ? toast.textContent : 'no toast');
      var undo = toast && all('button', toast).filter(function (b) { return /Undo/.test(b.textContent); })[0];
      if (undo) undo.click();
      return wait(300);
    })
    .then(function () {
      check('Undo brings it back', lastOp() === JSON.stringify({ op: 'update', id: 't-sample4', patch: { archived: false } }) && count() === '1', lastOp() + ' ' + count());
    })
    .then(done)
    .catch(stopped);
})();
