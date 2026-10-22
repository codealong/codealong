// Checks the Board view while the project has no boards (ui/BoardEmpty.tsx), as on a machine that
// has made none: the Kanban and the Canvas each say what a board is and offer New board…, with no
// board bar, columns, canvas or panel beside them; the sidebar lists no board under Board, whose
// menu offers only New board…; New board… opens the board dialog, and the board it makes is shown
// with its bar and listed under Board; deleted from its menu, the view says there are none again.
// With boards to start from, it first deletes them from the canvas, everything on it selected: the
// canvas's keys go with it, so Delete then takes nothing off a board that's gone.
//   scripts/preview.sh kanban '' scripts/preview/boards-empty-check.js
//   PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh canvas '' scripts/preview/boards-empty-check.js
(function () {
  var empty = function () { return q('.ca-boardempty'); };
  var shown = function () { var m = q('main.ca-board'); return m ? m.getAttribute('aria-label') : ''; };
  var subboards = function () { return all('.ca-side .ca-side-subboard:not(.ca-side-newboard)'); };
  var boardNav = function () {
    return all('.ca-side .ca-nav').filter(function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === 'Board'; })[0];
  };
  var key = function (k, code, mods) {
    var e = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true, key: k, code: code }, mods));
    (document.activeElement || document.body).dispatchEvent(e);
    return wait(150);
  };
  var press = function (letter) { return key(letter, 'Key' + letter, { metaKey: true, shiftKey: true }); };
  var type = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var removes = function () { return sent('board_op').filter(function (a) { return a.op.op === 'remove'; }); };
  // Deletes the board shown, from the menu on its name.
  var deleteShown = function () {
    var name = q('.ca-boardbar-name span').textContent;
    return menuOf(q('.ca-boardbar-name')).then(function () {
      menuItem('Delete ' + name)();
      return wait(300);
    });
  };
  // Under All projects every project's boards are listed: all but the last go first, then the
  // last with everything on its canvas selected.
  var fromBoards = function () {
    if (!q('.ca-boardbar')) return Promise.resolve();
    if (subboards().length > 1) return deleteShown().then(fromBoards);
    return until(function () { return q('.ca-cv'); }, 10000, 'the last board’s canvas')
      .then(function () { return key('a', 'KeyA', { metaKey: true }); })
      .then(deleteShown)
      .then(function () { return until(empty, 5000, 'the Canvas with no boards'); })
      .then(function () { return key('Backspace', 'Backspace', {}); })
      .then(function () {
        check('a canvas whose board went leaves no keys behind: Delete takes nothing off', removes().length === 0, JSON.stringify(removes()).slice(0, 160));
        return press('K');
      });
  };

  say('boards empty check: waiting');
  until(function () { return empty() || q('.ca-boardbar'); }, 20000, 'the Board view')
    .then(fromBoards)
    .then(function () { return until(function () { return shown() === 'Kanban' && empty(); }, 5000, 'the Kanban with no boards'); })
    .then(function () {
      check('the Kanban says there are no boards yet', /^No boards/.test(q('.ca-boardempty-title').textContent), q('.ca-boardempty-title').textContent);
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
      // Past the canvas's wait before it saves.
      var saves = calls('canvas_save').length;
      return wait(1500).then(function () {
        check('and saves no canvas', calls('canvas_save').length === saves, calls('canvas_save').length - saves + ' saves');
        q('.ca-boardempty .ca-btn').click();
        return until(function () { return q('.ca-boarddlg input'); }, 5000, 'the New board dialog');
      });
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
