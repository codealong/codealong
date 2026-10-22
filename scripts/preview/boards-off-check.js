// Checks that boards are an experiment (`[experimental] boards`, App.tsx `boardsOn`). Off, as a
// new config.toml has them, in a window that last saw them on: once the board comes the sidebar
// lists no Board, a window remembered on the Kanban shows Sessions, ⇧⌘K and ⇧⌘B go nowhere, a
// session's menu has no Add to board, the window remembers them off, and a canvas saved before
// isn't brought up to date and saved out of sight. On: the sidebar lists Board, the menu has Add to
// board, the canvas is kept up to date while another view shows, and ⇧⌘K shows the Kanban. Either
// way Settings › Experimental's Boards box saves the other way to config.toml.
//   PREVIEW_BOARDS_OFF=1 PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh kanban '' scripts/preview/boards-off-check.js
//   PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh sessions '' scripts/preview/boards-off-check.js
(function () {
  var off = !!window.__PREVIEW_BOARDS_OFF__;
  localStorage.setItem('codealong.boardsOn', '1');
  // A canvas saved empty, which the board's recent work would be brought onto (useCanvas.ts).
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'canvas_load') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ version: 1, items: [] });
    }
    return real(cmd, args, options);
  };
  var nav = function (root, label) {
    return all(root + ' .ca-nav').filter(function (b) { var l = b.querySelector('.ca-nav-label'); return l && l.textContent === label; })[0];
  };
  var shown = function () { var m = q('main.ca-board'); return m ? m.getAttribute('aria-label') : ''; };
  var rows = function () { return all('.ca-board-list [id^="row-"]'); };
  var press = function (letter) {
    var e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: letter, code: 'Key' + letter, metaKey: true, shiftKey: true });
    (document.activeElement || document.body).dispatchEvent(e);
    return wait(150);
  };
  var boardsBox = function () {
    var f = all('.ca-settings-pane .ca-field').filter(function (x) { var s = x.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === 'Boards'; })[0];
    return f ? f.querySelector('input[type=checkbox]') : null;
  };

  say('boards check: waiting');
  // A session row in Sessions says the board has come.
  until(function () { return nav('.ca-side', 'Sessions') && rows().length; }, 20000, 'a session row')
    .then(function () {
      check(off ? 'the sidebar lists no Board' : 'the sidebar lists Board', !nav('.ca-side', 'Board') === off, all('.ca-side .ca-nav-label').map(function (l) { return l.textContent; }).join(' | '));
      if (off) check('a window remembered on the Kanban shows Sessions', shown() === 'Sessions', shown());
      check('the window remembers them ' + (off ? 'off' : 'on'), localStorage.getItem('codealong.boardsOn') === (off ? '0' : '1'), localStorage.getItem('codealong.boardsOn'));
      return menuOf(rows()[0]).then(function (labels) {
        var adds = labels.some(function (l) { return l.indexOf('Add to board') === 0; });
        check(off ? 'a session’s menu has no Add to board' : 'a session’s menu has Add to board', adds !== off, labels.join(' | '));
      });
    })
    // Past the canvas's wait before it saves.
    .then(function () { return wait(1500); })
    .then(function () {
      var saves = calls('canvas_save').length;
      check(off ? 'the canvas isn’t brought up to date or saved' : 'the canvas is kept up to date while another view shows', (saves === 0) === off, saves + ' saves, ' + calls('canvas_load').length + ' loads');
    })
    .then(function () { return press('K'); })
    .then(function () {
      check(off ? '⇧⌘K goes nowhere' : '⇧⌘K shows the Kanban', shown() === (off ? 'Sessions' : 'Kanban'), shown());
      return press('B');
    })
    .then(function () {
      if (off) check('nor does ⇧⌘B', shown() === 'Sessions', shown());
      q('.ca-side-config').click();
      return until(function () { return nav('.ca-settings-nav', 'Experimental'); }, 10000, 'Settings › Experimental');
    })
    .then(function (section) {
      section.click();
      return until(boardsBox, 10000, 'the Boards box');
    })
    .then(function (box) {
      var was = box.checked;
      box.click();
      return until(function () { return calls('save_config').length; }, 5000, 'the box saved').then(function () {
        var saved = sent('save_config')[0].config.experimental;
        check('the Boards box saves ' + (was ? 'off' : 'on') + ' to [experimental] boards', !!saved && saved.boards === !was, JSON.stringify(saved));
      });
    })
    .then(done)
    .catch(stopped);
})();
