// Checks commands led by `!` in a session's panel, which run in a new tab of yours with the session
// told what they printed (actions.rs `run_command`): Run on a `!` block in a reply asks for that,
// with the block as written and the folder the session works in, says it's running, and says why
// under the block when it can't start; a command you ran with `!` before shows with what it
// printed and runs again the same way; and `!` in the box runs too, its button and the strip over
// the header saying so, ⏎, ⌃⏎ and ⌘⏎ running it, the box clearing, and having it back with why when it
// can't start. Needs a session whose newest page has a reply with a ``` block led by `!` and a
// command run with `!`:
//   PREVIEW_TRANSCRIPT=<key> PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/run-bang-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var fail = false;
  var STARTED = 'Running it in a new Ghostty tab. What it prints goes to the session when it ends.';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'run_command' || cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      if (cmd !== 'run_command') return Promise.resolve(cmd === 'send_to_session' ? { message: 'Sent.', opened: false } : null);
      return fail ? Promise.reject('Ghostty isn’t installed. Install it, or pick another terminal in Settings.') : Promise.resolve(STARTED);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return Promise.all([p, fetch('./transcript-key.json').then(function (r) { return r.json(); })]).then(function (all) {
      var board = all[0];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // A scratch session's folder may be one the board hides (a temporary one): list it.
          if (s.key === all[1].key) l.hidden = false;
          if (!picked && s.key === all[1].key && !s.visitor) {
            // Its turn ended, so the box takes a message; and it takes one into a turn at once,
            // so Send now shows for a message.
            s.status = 'idle';
            s.asks = false;
            s.steerable = true;
            picked = { key: s.key, lane: l.id, path: l.path, cwd: s.cwd };
          }
        });
      });
      return board;
    });
  };

  var rowOf = function (p) { return p && document.getElementById('row-' + p.lane + '::' + p.key); };
  var thread = function () { return document.querySelector('.ca-sessinsp .ca-thread'); };
  var bangs = function () {
    return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-agent .ca-codeblock')).filter(function (b) { return /^\s*!/.test(b.querySelector('pre').textContent); });
  };
  var runOf = function (b) { return b.querySelector('.ca-codeblock-tools button[aria-label^="Run in"]'); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var box = function () { return document.querySelector('.ca-sessinsp textarea'); };
  var sendButton = function () { return document.querySelector('.ca-sessinsp .ca-composer-send:not(.is-now)'); };
  // What the box says, in its foot.
  var under = function () { var n = document.querySelector('.ca-sessinsp .ca-composer-footrow > .ca-composer-note'); return n ? n.textContent : ''; };
  var press = function (opts) {
    box().dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key: 'Enter', bubbles: true, cancelable: true }, opts || {})));
  };
  var type = function (text) {
    var b = box();
    var set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    set.call(b, text);
    b.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // Where the panel says the session's commands run (RunButton.tsx `runFolder`).
  var folder = function () {
    var inside = picked.cwd === picked.path || picked.cwd.indexOf(picked.path.replace(/\/$/, '') + '/') === 0;
    return picked.cwd && inside ? picked.cwd : picked.path;
  };

  say('run-bang check: waiting for the list');
  until(function () { return rowOf(picked); }, 20000)
    .then(function () {
      rowOf(picked).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      rowOf(picked).click();
      return until(function () { return thread() && bangs().length > 0 && runOf(bangs()[bangs().length - 1]); }, 10000);
    })
    .then(function () {
      var b = bangs()[bangs().length - 1];
      var text = b.querySelector('pre').textContent;
      check('a block led by ! in a reply has Run', !!runOf(b));
      check('it runs in a tab of yours, and the session hears what it printed', /^Run in a new .+ tab in .+, as you: the session is told what it prints once it ends$/.test(runOf(b).title), runOf(b).title);
      runOf(b).click();
      return until(function () { return calls('run_command').length === 1; }, 3000)
        .then(function () {
          var args = calls('run_command')[0][1];
          check('Run asks to run the block as written, for the session', args.block === false && args.key === picked.key && args.text === text, JSON.stringify(args).slice(0, 160));
          check('in the folder the session works in', args.folder === folder(), args.folder + ' vs ' + folder());
          check('nothing is sent to the session', calls('send_to_session').length === 0);
          return until(function () { return runOf(b).classList.contains('is-done'); }, 2000);
        })
        .then(function () {
          check('it says it runs', /Running/.test(runOf(b).textContent), runOf(b).textContent);
          fail = true;
          return until(function () { return !runOf(b).classList.contains('is-done'); }, 4000);
        })
        .then(function () {
          runOf(b).click();
          return until(function () { return b.querySelector('.ca-codeblock-note'); }, 3000);
        })
        .then(function () {
          check('one that can’t start says why under its block', /isn’t installed/.test(b.querySelector('.ca-codeblock-note').textContent));
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
      check('its command runs again in a tab of yours', !!runOf(cmd.parentElement) && /as you: the session is told/.test(runOf(cmd.parentElement).title));
      check('its output has Copy alone', !!out && out.parentElement.querySelectorAll('.ca-codeblock-tools button').length === 1);
    })
    .then(function () {
      // `!` in the box runs it the same way, rather than sending it.
      type('! echo from the box');
      return until(function () { return sendButton() && sendButton().getAttribute('aria-label') === 'Run in your terminal'; }, 3000).then(function () {
        check('the box’s button runs it', /^Run it in a new .+ tab, as you \(⏎\)$/.test(sendButton().title), sendButton().title);
        check('no Send now beside it, which would send it into the turn', !document.querySelector('.ca-sessinsp .ca-composer-send.is-now'));
        check('the box’s foot says what happens', /runs it in a new .+ tab, as you; .+ is told what it prints once it ends/.test(under()), under());
        var before = calls('run_command').length;
        // ⏎ runs it, as the button does.
        press();
        return until(function () { return calls('run_command').length === before + 1; }, 3000).then(function () {
          var args = calls('run_command')[before][1];
          check('⏎ in the box asks to run it for the session, in its folder', args.block === false && args.key === picked.key && args.text === '! echo from the box' && args.folder === folder(), JSON.stringify(args).slice(0, 160));
          check('nothing is sent to the session', calls('send_to_session').length === 0);
          check('the box clears', box().value === '');
          return until(function () { return under().indexOf(STARTED) >= 0; }, 3000).then(function () {
            check('it says it runs', under().indexOf(STARTED) >= 0, under());
          });
        });
      });
    })
    .then(function () {
      fail = true;
      type('! make deploy');
      return until(function () { return sendButton().getAttribute('aria-label') === 'Run in your terminal'; }, 3000).then(function () {
        // ⌃⏎ (send now) and ⌘⏎ (send and go on) run it too, never sending it.
        var before = calls('run_command').length;
        press({ ctrlKey: true });
        return until(function () { return /isn’t installed/.test(under()); }, 3000).then(function () {
          check('⌃⏎ runs it too', calls('run_command').length === before + 1 && calls('send_to_session').length === 0);
          check('one that can’t start is back in the box, with why', box().value.indexOf('! make deploy') >= 0 && /isn’t installed/.test(under()), box().value + ' | ' + under());
          fail = false;
          press({ metaKey: true });
          return until(function () { return calls('run_command').length === before + 2; }, 3000).then(function () {
            check('⌘⏎ runs it too', calls('send_to_session').length === 0);
          });
        });
      });
    })
    .then(function () {
      // An image that opens the box is a message, not a command.
      type('![shot](images/ab12.png) what is this?');
      return wait(200).then(function () {
        check('an image’s ! sends as a message', sendButton().getAttribute('aria-label') === 'Send', sendButton().getAttribute('aria-label'));
        check('with Send now beside it, as a message has', !!document.querySelector('.ca-sessinsp .ca-composer-send.is-now'));
        type('');
      });
    })
    .then(done)
    .catch(stopped);
})();
