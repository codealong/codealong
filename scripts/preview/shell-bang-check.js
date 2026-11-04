// Checks ⌥⏎ on a command led by `!` in a session's box (SessionReply.tsx `handOver`): where the
// session's harness runs one in its own shell (a Claude Code terminal here, send.rs
// `shell_way_in`), the box's foot says ⏎ runs it in a terminal of yours and ⌥⏎ in its own shell,
// the button says so too, and ⌥⏎ or ⌥-click sends it to the session as written, running nothing
// here, the box clearing; where it doesn't (a Claude app session), the foot offers ⏎ alone, and ⌥⏎
// says why, keeping what's in the box.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/shell-bang-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var term = null;
  var app = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  // Its turn ended, so the box takes a message, and it runs here.
  var idle = function (s, surface) {
    s.status = 'idle';
    s.live = true;
    s.hosted = false;
    s.asks = false;
    s.surface = surface;
    s.attachCommand = undefined;
    s.suggestions = undefined;
    s.snoozedUntil = undefined;
    return s;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'send_to_session' || cmd === 'run_command') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(cmd === 'send_to_session' ? { message: 'Sent.', opened: false } : 'Running it.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      window.__madeSessions = window.__madeSessions || {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (term && s.key === term.key) idle(s, 'cli');
          if (app && s.key === app.key) idle(s, 'app');
          if (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          if (term && app) return;
          if (term && s.key === term.key) return;
          var made = idle(s, term ? 'app' : 'cli');
          window.__madeSessions[s.key] = made;
          if (!term) term = { key: s.key, lane: l.id };
          else if (!app) app = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var box = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var note = function () { var n = q('.ca-sessinsp .ca-composer-footrow > .ca-composer-note'); return n ? n.textContent : ''; };
  var button = function () { return q('.ca-sessinsp .ca-composer-send:not(.is-now)'); };
  var row = function (p) { return p && document.getElementById('row-' + p.lane + '::' + p.key); };
  var sent = function () { return window.__calls.filter(function (c) { return c[0] === 'send_to_session'; }); };
  var ran = function () { return window.__calls.filter(function (c) { return c[0] === 'run_command'; }); };
  var type = function (text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), text);
    box().dispatchEvent(new Event('input', { bubbles: true }));
  };
  var altEnter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', altKey: true, bubbles: true, cancelable: true })); };
  var open = function (p) {
    row(p).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    row(p).click();
    return until(function () { return !!box() && q('.ca-sessinsp') && q('.ca-sessinsp').textContent.length > 0; }, 5000, 'its panel');
  };

  say('shell-bang check: waiting for the list');
  until(function () { return row(term) && row(app); }, 20000, 'two Claude Code sessions to make a terminal one and an app one')
    .then(function () { return open(term); })
    .then(function () {
      type('! echo to its shell');
      return until(function () { return /⌥⏎/.test(note()); }, 4000, 'the foot offering ⌥⏎');
    })
    .then(function () {
      check('the foot says ⏎ runs it in a terminal of yours and ⌥⏎ in its own shell', /^⏎ runs it (in a terminal under the session|in a new .+ tab) · ⌥⏎ in .+’s own shell$/.test(note()), note());
      check('so does the button', /^Run it .+, as you \(⏎\): .+\. ⌥-click or ⌥⏎ runs it in .+’s own shell instead, as Claude Code’s ! does$/.test(button().title), button().title);
      altEnter();
      return until(function () { return sent().length === 1; }, 3000, '⌥⏎ sending it');
    })
    .then(function () {
      var args = sent()[0][1];
      check('⌥⏎ sends it to the session as written', args.key === term.key && args.text === '! echo to its shell', JSON.stringify(args).slice(0, 160));
      check('nothing runs here', ran().length === 0);
      check('the box clears', box().value === '', box().value);
      type('! echo by click');
      return until(function () { return /⌥-click/.test(button().title); }, 3000, 'the button offering ⌥-click');
    })
    .then(function () {
      button().dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, altKey: true }));
      return until(function () { return sent().length === 2; }, 3000, '⌥-click sending it');
    })
    .then(function () {
      check('⌥-click sends it too', sent()[1][1].text === '! echo by click' && ran().length === 0, JSON.stringify(sent()[1][1]).slice(0, 120));
      return open(app);
    })
    .then(function () {
      type('! echo nowhere');
      return until(function () { return /^⏎ runs it/.test(note()); }, 4000, 'the foot saying what ⏎ does');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('a Claude app session’s foot offers ⏎ alone', /prints once it ends\.$/.test(note()) && !/⌥⏎/.test(note()), note());
      altEnter();
      return until(function () { return /can’t hand/.test(note()); }, 3000, '⌥⏎ saying why');
    })
    .then(function () {
      check('⌥⏎ there says Codealong can’t hand it to its shell, and what ⏎ does', /^Codealong can’t hand .+ a command to run in its own shell from here; ⏎ runs it .+, as you\.$/.test(note()), note());
      check('it keeps what’s in the box and sends nothing', box().value === '! echo nowhere' && sent().length === 2, box().value);
    })
    .then(done)
    .catch(stopped);
})();
