// A session's menu in the todo panel says whether it's the todo's primary session, ticked when it
// is, and the todo panel has no start button while that session runs, its chip showing it.
//
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/todo-primary-menu-check.js
//
// The sample's first todo names the project's latest session as its primary one; the check needs
// that session to be running.
until(function () {
  return q('.ca-todoinsp .ca-places');
}, 30000)
  .then(function (places) {
    // The panel names its sessions as chips, the primary one saying so.
    var row = all('.ca-chip', places).filter(function (c) { return !!q('.ca-places-primary', c); })[0];
    if (!row) return stopped('the sample todo’s primary session isn’t running on this board');
    var buttons = all('.ca-todoinsp-actions .ca-btn').map(function (b) { return b.textContent.trim(); });
    check('the panel has no start button while its primary session runs: its chip shows it', buttons.length === 0, buttons.join(' | '));
    rightClick(row);
    return wait(100).then(function () {
      var labels = menuLabels();
      var item = labels.filter(function (l) { return l.indexOf('Primary session for') === 0; });
      check('its menu says it’s the todo’s primary session', item.length === 1 && / ✓$/.test(item[0]), labels.join(' | '));
      done();
    });
  })
  .catch(stopped);
