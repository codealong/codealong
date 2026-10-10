// Checks Run on a block led by `!` in a session's conversation, a command an agent wrote for you
// to run in the session's own shell: in a Claude Code session Codealong types into, Run sends it
// to the session as a reply (the thread shows it on its way), a send that fails says why under
// the block, and a command you ran that way shows with what it printed. Where the session can't
// take it, Run types it into a new terminal without its `!`. Needs a live Claude Code terminal
// session whose newest page has a reply with a ``` block led by `!` and a command run with `!`:
//   PREVIEW_TRANSCRIPT=<key> PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/run-bang-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var other = null;
  var fail = false;
  // The way the engine says a message goes, once the check changes it.
  var way = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'run_in_terminal') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(null);
    }
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return fail ? Promise.reject('“Bang smoke” runs a command that starts with ! only when it’s typed into its own box.') : Promise.resolve({ message: 'Typed into it.', opened: false });
    }
    if (cmd === 'session_way' && way) return Promise.resolve(way);
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return Promise.all([p, fetch('./transcript-key.json').then(function (r) { return r.json(); })]).then(function (all) {
      var board = all[0];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // A scratch session's folder may be one the board hides (a temporary one): list it.
          if (s.key === all[1].key) l.hidden = false;
          if (!picked && s.key === all[1].key && !s.visitor) picked = { key: s.key, lane: l.id, path: l.path, cwd: s.cwd };
          else if (!other && !s.visitor) other = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var rowOf = function (p) { return p && document.getElementById('row-' + p.lane + '::' + p.key); };
  var open = function (p) {
    rowOf(p).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    rowOf(p).click();
  };
  var thread = function () { return document.querySelector('.ca-sessinsp .ca-thread'); };
  var bangs = function () {
    return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-agent .ca-codeblock')).filter(function (b) { return /^\s*!/.test(b.querySelector('pre').textContent); });
  };
  var runOf = function (b) { return b.querySelector('.ca-codeblock-tools button[aria-label^="Run in"]'); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };

  say('run-bang check: waiting for the list');
  until(function () { return rowOf(picked); }, 20000)
    .then(function () {
      open(picked);
      return until(function () { return thread() && bangs().length > 0 && runOf(bangs()[bangs().length - 1]); }, 10000);
    })
    .then(function () {
      var b = bangs()[bangs().length - 1];
      var text = b.querySelector('pre').textContent;
      check('a block led by ! in a reply has Run', !!runOf(b));
      check('it runs in the session', runOf(b).title === 'Run in this session: its agent reads what it prints', runOf(b).title);
      runOf(b).click();
      return until(function () { return calls('send_to_session').length === 1; }, 3000)
        .then(function () {
          var args = calls('send_to_session')[0][1];
          check('Run sends the command, led by !, to the session', args.key === picked.key && args.text === text.trim() && !args.now, JSON.stringify(args).slice(0, 160));
          check('nothing opens a terminal', calls('run_in_terminal').length === 0);
          return until(function () { return runOf(b).classList.contains('is-done'); }, 2000);
        })
        .then(function () {
          check('it says it went', /Sent/.test(runOf(b).textContent));
          check('the thread shows it on its way', Array.prototype.some.call(document.querySelectorAll('.ca-sessinsp .ca-thread-said.is-ahead, .ca-sessinsp .ca-thread-said.is-going'), function (el) { return el.textContent.indexOf(text.trim()) >= 0; }));
          fail = true;
          return until(function () { return !runOf(b).classList.contains('is-done'); }, 4000);
        })
        .then(function () {
          runOf(b).click();
          return until(function () { return b.querySelector('.ca-codeblock-note'); }, 3000);
        })
        .then(function () {
          check('a send that fails says why under its block', /only when it’s typed into its own box/.test(b.querySelector('.ca-codeblock-note').textContent));
          check('and leaves the reply box alone', !/!/.test((document.querySelector('.ca-sessinsp textarea') || { value: '' }).value));
          fail = false;
        });
    })
    .then(function () {
      // A command you ran with `!`: the command, with Run and Copy, and what it printed, with Copy.
      var ran = Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-ran'));
      check('a command run with ! shows in the conversation', ran.length > 0, ran.length + ' of them');
      if (!ran.length) return;
      var last = ran[ran.length - 1];
      var cmd = last.querySelector('.ca-thread-in');
      var out = last.querySelector('.ca-thread-out');
      check('as yours', last.closest('.ca-thread-said').classList.contains('is-you'));
      check('with what it printed', !!out && out.textContent.trim().length > 0, out ? out.textContent.slice(0, 60) : 'none');
      check('its command can run again in the session', !!runOf(cmd.parentElement) && /this session/.test(runOf(cmd.parentElement).title));
      check('its output has Copy alone', !!out && out.parentElement.querySelectorAll('.ca-codeblock-tools button').length === 1);
      // Its own Copy is the one: a right-click on it has no Copy message.
      var menusBefore = menus();
      rightClick(cmd);
      return wait(200).then(function () {
        check('no Copy message beside its own Copy', menus() === menusBefore || menuLabels().indexOf('Copy message') < 0, menuLabels().join(' | '));
      });
    })
    .then(function () {
      // A session Codealong can only message: the block runs in a new terminal, without its `!`.
      way = { kind: 'message' };
      if (!other) {
        check('another session to step away to', false);
        return;
      }
      open(other);
      return until(function () { return !thread() || bangs().length === 0; }, 5000)
        .then(function () {
          open(picked);
          return until(function () { var bs = bangs(); return bs.length > 0 && runOf(bs[bs.length - 1]) && runOf(bs[bs.length - 1]).title.indexOf('Run in this session') < 0; }, 10000);
        })
        .then(function () {
          var b = bangs()[bangs().length - 1];
          var text = b.querySelector('pre').textContent.trim();
          check('elsewhere it runs in a new terminal', /^Run in .+, in /.test(runOf(b).title), runOf(b).title);
          var before = calls('run_in_terminal').length;
          runOf(b).click();
          return until(function () { return calls('run_in_terminal').length === before + 1; }, 3000).then(function () {
            var args = calls('run_in_terminal')[before][1];
            check('without its !', args.text === text.replace(/^!\s*/, ''), JSON.stringify(args).slice(0, 160));
          });
        });
    })
    .then(done)
    .catch(stopped);
})();
