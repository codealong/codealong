// Checks Run on a block with no language in a reply: one whose lines each start with a command on
// this Mac (shell-commands.json, the engine's list) gets Run beside Copy and runs in a new
// terminal where the session works, for the session; one of output or a path has Copy alone; and
// every block in the window shares one ask for the list. The reply is made up here, on the first
// session of this Mac's that the list shows:
//   scripts/preview.sh sessions '' scripts/preview/run-untagged-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var command = 'git -C ~/projects/app pull --ff-only';
  var reply = 'To update it:\n\n```\n' + command + '\n```\n\nIt printed:\n\n```\nAlready up to date.\n```\n\nThe notes are in\n\n```\n~/projects/app/NOTES.md\n```';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'run_command') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Running it in a new Ghostty tab. What it prints goes to the session when it ends.');
    }
    if (cmd === 'session_page' && picked && args && args.key === picked.key) {
      var at = Date.now() - 60e3;
      return Promise.resolve({ before: null, after: 2, items: [
        { at: at, kind: 'you', text: 'Is my checkout up to date?', failed: false, cursor: 0, n: 0 },
        { at: at + 1e3, kind: 'agent', text: reply, failed: false, cursor: 1, n: 0 },
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.hosted) picked = { key: s.key, lane: l.id, path: l.path, cwd: s.cwd };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var blocks = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-agent .ca-codeblock')); };
  var runOf = function (b) { return b.querySelector('.ca-codeblock-tools button[aria-label^="Run in"]'); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var tick = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };

  say('run-untagged check: waiting for the list');
  fetch('./shell-commands.json')
    .then(function (r) { return r.json(); })
    .then(function (names) {
      check('the engine lists git and cd among the commands', names.indexOf('git') >= 0 && names.indexOf('cd') >= 0, names.length + ' commands');
      check('and no path or output', names.every(function (n) { return n.indexOf('/') < 0; }) && names.indexOf('Already') < 0);
      return until(function () { return row(); }, 20000);
    })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return blocks().length === 3 && runOf(blocks()[0]); }, 10000, 'no Run on the command');
    })
    .then(function () { return tick(300); })
    .then(function () {
      var b = blocks();
      check('a block with no language that holds a command has Run', !!runOf(b[0]));
      check('Run comes before Copy', b[0].querySelectorAll('.ca-codeblock-tools button').length === 2 && b[0].querySelectorAll('.ca-codeblock-tools button')[1].getAttribute('aria-label') === 'Copy');
      check('output has Copy alone', !runOf(b[1]) && b[1].querySelectorAll('.ca-codeblock-tools button').length === 1);
      check('a path has Copy alone', !runOf(b[2]) && b[2].querySelectorAll('.ca-codeblock-tools button').length === 1);
      check('the blocks ask for the list once', calls('shell_commands').length === 1, calls('shell_commands').length + ' asks');
      var want = picked.cwd && (picked.cwd === picked.path || picked.cwd.indexOf(picked.path.replace(/\/$/, '') + '/') === 0) ? picked.cwd : picked.path;
      runOf(b[0]).click();
      return until(function () { return calls('run_command').length === 1; }, 3000).then(function () {
        var args = calls('run_command')[0][1];
        check('Run runs the command in a new terminal where the session works, for the session', args.block === true && args.key === picked.key && args.text === command && args.folder === want, JSON.stringify(args).slice(0, 160));
      });
    })
    .then(done)
    .catch(stopped);
})();
