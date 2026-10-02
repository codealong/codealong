// Checks a session its account's usage limit stopped (Session.limit, start.rs `limit_moves`), in
// the bar's peek: its header says which limit and when it lifts, and offers the move to the
// account with room as a button, which asks for move_session with that account; its menu's Move
// to says it goes on. The limit is put on a real Claude Code session, since a board rarely has one.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/limit-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };

  // A Claude Code session that resumes in a terminal, on an account with one other here.
  var board = get('./board.json');
  var accounts = get('./accounts.json').filter(function (a) { return a.harness === 'claude' && a.signedIn; });
  var picked = null;
  var other = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || s.archived || s.harness !== 'claude' || !s.resumeCommand) return;
      var others = accounts.filter(function (a) { return a.name !== s.account; });
      if (others.length === 1) { picked = s; other = others[0].name; }
    });
  });
  if (!picked) { stopped('no Claude Code session with exactly one other Claude Code account to move to'); return; }
  var key = picked.key;
  var lifts = Date.now() + 3 * 3600 * 1000 + 10 * 60 * 1000;
  localStorage.setItem('codealong.preview.peek', key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'move_session') {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.resolve({ key: 'claude:' + other + ':f0000000-0000-4000-8000-000000000000', message: 'Moved “x” to ' + other + ': it carries on in a new terminal in y.' });
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.key !== key) return;
          s.status = 'error';
          s.excerpt = 'You’ve hit your monthly spend limit · raise it at claude.ai/settings/usage';
          s.limit = { meter: 'week', resetsAt: lifts, spendCapped: true, at: Date.now() - 60000, moveTo: other };
        });
      });
      return b;
    });
  };

  (async function () {
    try {
      var line = await until(function () { return document.querySelector('.ca-sessinsp-head .ca-sessinsp-limit'); }, 20000);
      var words = line.querySelector('.ca-sessinsp-limit-words').textContent;
      check('its header says which limit stopped it', words.indexOf(picked.account + '’s weekly limit') >= 0, words);
      check('and when it lifts', /It lifts in 3h \d+m\./.test(words), words);
      check('and leaves the move to the button', words.indexOf('has room') < 0, words);
      var button = line.querySelector('button');
      check('it offers the move to the account with room', button && button.textContent === 'Move to ' + other + ' and go on', button ? button.textContent : 'no button');
      var more = document.querySelector('.ca-sessinsp-head [aria-label="More"]');
      if (more) {
        var n = menus();
        more.click();
        await until(function () { return menus() > n; }, 3000);
        var labels = window.__menu().map(function (x) { return x[0]; });
        check('its menu says a move goes on', labels.indexOf('Move to ' + other + ' and go on') >= 0, labels.join(' | '));
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      } else {
        note('no ⋯ button in the peek to read its menu from');
      }
      if (button) {
        button.click();
        await until(function () { return calls('move_session').length > 0; }, 3000);
        var asked = calls('move_session')[0][1];
        check('the button asks to move it there', asked.key === key && asked.account === other, JSON.stringify(asked));
      }
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
