// Checks Move to in a session's menu (menus.ts), in the bar's peek: a Claude Code session whose
// account has another signed-in Claude Code account here lists Move to that account; picking it
// asks for move_session with the session and the account, and says what happened. And a session
// forked from one on another account (a move, start.rs `move_to`) says Moved from over its title.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/move-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };

  // A Claude Code session that resumes in a terminal, on an account with another one here.
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
  // One on the other account, to have been moved from.
  var from = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!from && s.harness === 'claude' && s.account === other) from = s; }); });
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'move_session') {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.resolve({ key: 'claude:' + other + ':f0000000-0000-4000-8000-000000000000', message: 'Moved “x” to ' + other + ': it goes on in a new terminal in y.' });
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board' || !from) return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) s.origin = { key: from.key, how: 'fork' }; }); });
      return b;
    });
  };

  (async function () {
    try {
      var more = await until(function () { return document.querySelector('.ca-sessinsp-head [aria-label="More"]'); }, 20000);
      if (from) {
        var phrase = await until(function () { return document.querySelector('.ca-sessinsp-kin.is-parent .ca-sessinsp-kin-phrase'); }, 5000).catch(function () { return null; });
        check('a session moved from another account says so', phrase && phrase.textContent === 'Moved from', phrase ? phrase.textContent : 'no origin line');
      }
      var n = menus();
      more.click();
      await until(function () { return menus() > n; }, 3000);
      var items = window.__menu();
      var labels = items.map(function (x) { return x[0]; });
      var move = items.filter(function (x) { return x[0] === 'Move to ' + other; })[0];
      check('its menu lists Move to ' + other, !!move, labels.join(' | '));
      check('right after Fork in terminal', labels.indexOf('Move to ' + other) === labels.indexOf('Fork in terminal') + 1, labels.join(' | '));
      check('and not its own account', labels.indexOf('Move to ' + picked.account) < 0);
      if (move) {
        move[1]();
        await until(function () { return calls('move_session').length > 0; }, 3000);
        var asked = calls('move_session')[0][1];
        check('picking it asks to move the session there', asked.key === key && asked.account === other, JSON.stringify(asked));
        await wait(300);
        check('and says what happened', document.body.textContent.indexOf('Moved “x” to ' + other) >= 0 || calls('peek_hide').length > 0);
      }
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
