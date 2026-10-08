// The quick panel ready to start a todo whose primary session is running: the start row leads
// with that session, and the todo's strip names it, as a flag that shows it.
//
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh quick '' scripts/preview/quick-todo-primary-check.js
//
// The sample's first todo names the project's latest session as its primary one; the check needs
// that session to be running.
until(function () {
  return calls('quick_ready').length > 0;
}, 30000)
  .then(function () {
    return window.__TAURI_INTERNALS__.invoke('get_board');
  })
  .then(function (board) {
    var todo = board.todos.find(function (t) { return t.id === 't-sample1'; });
    var primary = null;
    board.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) { if (todo && todo.primary && s.key === todo.primary.session && !s.visitor) primary = s; });
    });
    if (!primary || !primary.live || primary.archived) return stopped('the sample todo’s primary session isn’t running on this board');
    window.__quickOpen({ todo: 't-sample1' }, board);
    return wait(300).then(function () {
      var why = q('.ca-quick-compose .ca-quick-row-why');
      var text = why ? why.textContent : '';
      check('the start row leads with the running primary session', text.indexOf('“' + primary.title + '” (' + primary.account) === 0, text.slice(0, 120));
      check('it says the new session works alongside it', text.indexOf('primary session and is running: this one would work alongside it.') > 0, text.slice(0, 160));
      var flag = q('.ca-quick-todo-primary');
      check('the todo’s strip names it', !!flag && flag.textContent.indexOf(primary.title) >= 0, flag ? flag.textContent : 'no flag');
      var input = q('.ca-quick-input textarea, textarea');
      var prompt = input ? input.value : '';
      check('the prompt tells the new session to send it what it finds', prompt.indexOf("t-sample1's primary session is " + primary.key + ', which is running') >= 0, prompt.slice(-200));
      // Clicking the flag shows the primary session in Codealong.
      flag.click();
      return wait(100).then(function () {
        var shown = calls('show_main').map(function (c) { return JSON.stringify(c[1]); });
        check('clicking the flag shows that session in Codealong', shown.some(function (a) { return a.indexOf(primary.key) >= 0; }), shown.join(' | ') || 'nothing shown');
        done();
      });
    });
  })
  .catch(stopped);
