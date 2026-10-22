// Checks the canvas's toolbar now that the board's settings say what's on it: there's no Tidy or
// Bring in panel; Tidy offers its two layouts, the last one ticked, and one picked lays the canvas
// out at once and keeps that layout; the gear and a right-click's Board settings… open the
// board's settings; and a right-click on the canvas has no Clear canvas.
//   PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh canvas '' scripts/preview/canvas-tools-check.js
(function () {
  var tool = function (label) {
    return all('.ca-cv-toolbar button').filter(function (b) { return b.textContent.trim() === label || b.getAttribute('aria-label') === label; })[0];
  };
  var clickMenu = function (el) {
    var before = menus();
    el.click();
    return until(function () { return menus() > before; }, 3000, 'no menu').then(menuLabels);
  };
  var saved = function () { return last('canvas_save'); };
  var dialogTitle = function () { var h = q('.ca-boarddlg .ca-dialog-title, .ca-boarddlg h2'); return h ? h.textContent : ''; };
  var closeDialog = function () {
    var cancel = all('.ca-boarddlg button').filter(function (b) { return b.textContent.trim() === 'Cancel'; })[0];
    if (cancel) cancel.click();
    return wait(200);
  };

  until(function () { return q('.ca-cv-toolbar'); }, 8000, 'no canvas')
    .then(function () {
      check('no Tidy or Bring in panel', !q('.ca-cv-tidy'));
      check('no Bring in button', !tool('Bring in'));
      return clickMenu(tool('Tidy'));
    })
    .then(function (labels) {
      check('Tidy offers its two layouts, one ticked', labels.length === 2 && labels.filter(function (l) { return / ✓$/.test(l); }).length === 1, labels.join(' | '));
      check('and no panel opened', !q('.ca-cv-tidy'));
      var byDay = menuItem('Tidy by day') || menuItem('Tidy by day ✓');
      byDay();
      return until(function () { var d = saved(); return d && d.doc && d.doc.layout === 'time'; }, 3000, 'not saved by day');
    })
    .then(function () {
      var doc = saved().doc;
      check('the saved canvas has no settings of its own', !('scope' in doc) && !('tidy' in doc), Object.keys(doc).join(', '));
      var titles = all('.ca-cv-frame-title').map(function (t) { return t.textContent; });
      note('frames by day: ' + titles.join(', '));
      return clickMenu(tool('Tidy'));
    })
    .then(function (labels) {
      check('Tidy by day is ticked now', labels.indexOf('Tidy by day ✓') >= 0, labels.join(' | '));
      tool('Board settings').click();
      return until(function () { return q('.ca-boarddlg'); }, 3000, 'no board settings');
    })
    .then(function () {
      check('the gear opens Board settings', /Board settings/.test(dialogTitle()), dialogTitle());
      return closeDialog();
    })
    .then(function () {
      var r = q('.ca-cv').getBoundingClientRect();
      return menuOf(q('.ca-cv'), { clientX: r.left + 40, clientY: r.top + 40 });
    })
    .then(function (labels) {
      check('a right-click on the canvas tidies and opens the settings', labels.indexOf('Tidy by worktree') >= 0 && labels.indexOf('Tidy by day ✓') >= 0 && labels.indexOf('Board settings…') >= 0, labels.join(' | '));
      check('and has no Clear canvas', labels.indexOf('Clear canvas') < 0 && labels.indexOf('Tidy…') < 0);
      menuItem('Board settings…')();
      return until(function () { return q('.ca-boarddlg'); }, 3000, 'no board settings from the menu');
    })
    .then(closeDialog)
    .then(done, stopped);
})();
