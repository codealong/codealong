// Checks the Board view while the project has no boards (ui/BoardEmpty.tsx), as on a machine that
// has made none: the Kanban and the Canvas each say what a board is and offer New board…, with no
// board bar, columns, canvas or panel beside them; the sidebar lists no board under Board, whose
// menu offers only New board…; New board… opens the board dialog, and the board it makes is shown
// with its bar and listed under Board; deleted from its menu, the view says there are none again.
//   scripts/preview.sh kanban '' scripts/preview/boards-empty-check.js
(function () {
  var empty = function () { return q('.ca-boardempty'); };
  var shown = function () { var m = q('main.ca-board'); return m ? m.getAttribute('aria-label') : ''; };
  var subboards = function () { return all('.ca-side .ca-side-subboard:not(.ca-side-newboard)'); };
  var boardNav = function () {
    return all('.ca-side .ca-nav').filter(function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === 'Board'; })[0];
  };
  var press = function (letter) {
    var e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: letter, code: 'Key' + letter, metaKey: true, shiftKey: true });
    (document.activeElement || document.body).dispatchEvent(e);
    return wait(150);
  };
  var type = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  say('boards empty check: waiting');
  until(empty, 20000, 'the Board view with no boards')
    .then(function () {
      check('the Kanban says there are no boards yet', shown() === 'Kanban' && /^No boards/.test(q('.ca-boardempty-title').textContent), shown() + ': ' + q('.ca-boardempty-title').textContent);
      check('and what a board is', /A board is a set of work/.test(q('.ca-boardempty-text').textContent));
      check('with no board bar and no columns', !q('.ca-boardbar') && !q('.ca-kb-col'));
      check('and no panel beside it, as there’s nothing to select', !q('.ca-insp') && q('.ca-win-body').classList.contains('is-boardless'));
      check('the sidebar lists no board under Board', subboards().length === 0, subboards().map(function (b) { return b.textContent; }).join(' | '));
      return menuOf(boardNav());
    })
    .then(function (labels) {
      check('Board’s menu offers only New board…', labels.join('|') === 'New board…', labels.join(' | '));
      return press('B');
    })
    .then(function () { return until(function () { return shown() === 'Canvas' && empty(); }, 5000, 'the Canvas with no boards'); })
    .then(function () {
      check('the Canvas says so too, with no canvas', !q('.ca-cv') && !q('.ca-boardbar'));
      check('and saves no canvas', calls('canvas_save').length === 0, calls('canvas_save').length + ' saves');
      q('.ca-boardempty .ca-btn').click();
      return until(function () { return q('.ca-boarddlg input'); }, 5000, 'the New board dialog');
    })
    .then(function (input) {
      check('New board… opens the board dialog', /New board/.test(q('.ca-boarddlg').textContent));
      type(input, 'Release');
      return wait(100).then(function () {
        all('.ca-boarddlg button').filter(function (b) { return b.textContent === 'Make board'; })[0].click();
        return until(function () { return q('.ca-boardbar-name'); }, 5000, 'the board made');
      });
    })
    .then(function () {
      var made = sent('board_op').filter(function (a) { return a.op.op === 'create'; })[0];
      check('it makes the board', !!made && made.op.board.name === 'Release', JSON.stringify(made).slice(0, 160));
      check('and shows it, with its bar', q('.ca-boardbar-name').textContent === 'Release' && !empty());
      check('listed under Board', subboards().some(function (b) { return /Release/.test(b.textContent); }), subboards().map(function (b) { return b.textContent; }).join(' | '));
      return menuOf(subboards()[0]);
    })
    .then(function (labels) {
      check('its menu can delete it', labels.indexOf('Delete Release') >= 0, labels.join(' | '));
      menuItem('Delete Release')();
      return until(empty, 5000, 'the Board view with no boards again');
    })
    .then(function () {
      check('deleted, the view says there are none again', subboards().length === 0 && !q('.ca-boardbar'));
    })
    .then(done)
    .catch(stopped);
})();
