// Checks Run on code blocks in a session's conversation: a shell block in a reply and a shell
// command the agent ran get Run beside Copy, other blocks don't, and Run sends the block's text
// with the folder it runs in (the session's worktree, or where the command ran); a failed Run says
// why under its block. Needs a session whose newest page has a ```bash block in a reply and a
// shell call:
//   PREVIEW_TRANSCRIPT=<key> PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/run-code-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var fail = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'run_in_terminal') {
      window.__calls.push([cmd, args, options]);
      return fail ? Promise.reject("Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.") : Promise.resolve(null);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return Promise.all([p, fetch('./transcript-key.json').then(function (r) { return r.json(); })]).then(function (all) {
      var board = all[0];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && s.key === all[1].key && !s.visitor) picked = { key: s.key, lane: l.id, path: l.path, cwd: s.cwd };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var thread = function () { return document.querySelector('.ca-sessinsp .ca-thread'); };
  var blocks = function (sel) { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp ' + sel + ' .ca-codeblock')); };
  var runOf = function (b) { return b.querySelector('.ca-codeblock-tools button[aria-label^="Run in"]'); };
  var tick = function (ms) { return new Promise(function (r) { setTimeout(r, ms || 30); }); };
  var calls = function () { return window.__calls.filter(function (c) { return c[0] === 'run_in_terminal'; }); };

  say('run check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return thread() && blocks('.ca-thread-agent').length > 0; }, 10000);
    })
    .then(function () {
      var inReplies = blocks('.ca-thread-agent');
      var runs = inReplies.filter(runOf);
      check('a reply has a shell block with Run', runs.length > 0, runs.length + ' of ' + inReplies.length);
      // Another language's block, or one with no language that isn't commands, has Copy alone
      // (run-untagged-check.js checks which are).
      var plain = inReplies.filter(function (b) { return !runOf(b); });
      check('other blocks have Copy alone', plain.every(function (b) { return b.querySelectorAll('.ca-codeblock-tools button').length === 1; }));
      check('Run comes before Copy', runs.every(function (b) { var bs = b.querySelectorAll('.ca-codeblock-tools button'); return bs.length === 2 && bs[0] === runOf(b) && bs[1].getAttribute('aria-label') === 'Copy'; }));
      var want = picked.cwd && (picked.cwd === picked.path || picked.cwd.indexOf(picked.path.replace(/\/$/, '') + '/') === 0) ? picked.cwd : picked.path;
      var b = runs[0];
      check('its title says where it runs', runOf(b).title.indexOf('Run in ') === 0 && runOf(b).title.indexOf(want.replace(/^\/Users\/[^/]+/, '~')) > 0, runOf(b).title);
      runOf(b).click();
      return until(function () { return calls().length === 1; }, 3000).then(function () {
        var args = calls()[0][1];
        check('Run sends the block and the session\'s folder', args.text === b.querySelector('pre').textContent && args.cwd === want, JSON.stringify(args).slice(0, 160));
        return until(function () { return runOf(b).classList.contains('is-done'); }, 2000).then(function () {
          check('it says it opened', /Opened/.test(runOf(b).textContent));
        });
      }).then(function () {
        fail = true;
        return until(function () { return !runOf(b).classList.contains('is-done'); }, 4000).then(function () {
          runOf(b).click();
          return until(function () { return b.querySelector('.ca-codeblock-note'); }, 3000);
        });
      }).then(function () {
        check('a failed Run says why under its block', /isn't allowed to control Ghostty/.test(b.querySelector('.ca-codeblock-note').textContent));
        fail = false;
      });
    })
    .then(function () {
      // A shell call's command, opened: Run sends the folder it ran in, when its record says.
      var heads = Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-toolhead'));
      var shellHead = heads.filter(function (h) { return /^(Bash|Shell) · /.test(h.textContent.trim()); }).pop();
      check('the page has a shell call', !!shellHead);
      if (!shellHead) return;
      shellHead.click();
      return until(function () { return shellHead.parentElement.querySelector('.ca-thread-in'); }, 3000).then(function () {
        var block = shellHead.parentElement.querySelector('.ca-thread-in').parentElement;
        check('a shell call\'s command has Run', !!runOf(block));
        var out2 = shellHead.parentElement.querySelector('.ca-thread-out');
        check('its output has Copy alone', !out2 || out2.parentElement.querySelectorAll('.ca-codeblock-tools button').length === 1);
        var before = calls().length;
        runOf(block).click();
        return until(function () { return calls().length === before + 1; }, 3000).then(function () {
          var args = calls()[before][1];
          check('Run sends the command with the folder it ran in', args.text === block.querySelector('pre').textContent && typeof args.cwd === 'string' && args.cwd.indexOf('/') === 0, JSON.stringify(args).slice(0, 160));
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
