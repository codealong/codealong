// Checks Mark as done from the Sessions view's panel (menus.ts `sessionActions`, mainKeys.ts):
// three sessions wait on you and one runs. ⇧⌘E from the box marks the first done: the engine is
// asked, it leaves the list at once and the panel goes on to the next, the strip says how it
// went and the toast offers Undo, which unarchives it. The next, which the engine holds (a
// command left running in the background), comes back, says why, and ⇧⌘E again ends that too.
// One that's working is held here without asking the engine, and Done on the strip, which says
// its key, then ⇧⌘E again ends it too. The first and the working one hold worktrees their harness
// made for them: Done's tooltip says the first exits its harness's way, removing the worktree or
// asking about it, and that the working one's terminal only closes, leaving it.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/done-check.js
(function () {
  // This runs before the app's script: the board it gets has three sessions waiting on you and
  // one running, made from real ones on this device, and nothing else waiting.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'unread', since: now - 30 * 60e3 },
    { status: 'unread', since: now - 20 * 60e3 },
    { status: 'unread', since: now - 10 * 60e3 },
    { status: 'working', since: now - 60e3 },
  ];
  var picked = [];
  // The engine holds the second session it's asked about, the first time, as one that left a
  // command running in the background.
  var asks = 0;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_done') {
      window.__calls.push([cmd, args, options]);
      var name = 'Done ' + 'ABCW'[picked.findIndex(function (p) { return p.key === args.key; })];
      if (++asks === 2) return Promise.resolve({ done: false, message: '“' + name + '” left `npm run dev` running in the background.' });
      return Promise.resolve({ done: true, message: 'Done with “' + name + '”: its terminal is closed and it’s archived.', at: null });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.live = true;
          s.surface = 'cli';
          s.pid = 4242 + picked.length;
          s.title = 'Done ' + 'ABCW'[picked.length];
          s.ownWorktree = picked.length === 0 || picked.length === 3 ? '/r/app/.claude/worktrees/brave-otter' : undefined;
          s.background = undefined;
          s.attachCommand = null;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var toast = function () { var t = document.querySelector('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var listed = function (i) { var r = row(i); return !!r && !r.closest('.ca-archived'); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var doneKey = function () { key({ key: 'E', code: 'KeyE', metaKey: true, shiftKey: true }); };
  var asked = function () { return window.__calls.filter(function (c) { return c[0] === 'session_done'; }).map(function (c) { return c[1]; }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var hotkey = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  // The one gone on to after the first, by its place in `picked`.
  var next = -1;

  say('done check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Done A').then(function () { return wait(50); });
    })
    .then(function () {
      check('the cursor is in the box', document.activeElement === box());
      var b = hotkey('Done');
      var tip = b ? b.title : '';
      check('Done’s tooltip says its harness exits it and removes its worktree, or asks', /exits it, removing the worktree it made for it \(brave-otter\), or asking in its terminal whether to keep it/.test(tip), tip);
      doneKey();
      return until(function () { return title() !== 'Done A'; }, 3000, 'the next').then(function () { return wait(100); });
    })
    .then(function () {
      var a = asked()[0];
      next = 'ABC'.indexOf((title() || '').slice(-1));
      check('⇧⌘E asks the engine to be done with it', !!a && a.key === picked[0].key && a.stop === false, JSON.stringify(a));
      check('and goes on to the next that waits', next > 0, title());
      check('the strip says how it went', /Done A: done\./.test(strip()), strip());
      check('it leaves the list at once', !listed(0));
      check('the toast says what was done, with Undo', /Done with “Done A”: its terminal is closed/.test(toast()) && /Undo/.test(toast()), toast());
      check('the next one’s box has the cursor', document.activeElement === box());
      // Undo takes it out of the archive.
      var undo = document.querySelector('.ca-toast .ca-toast-action');
      if (undo) undo.click();
      return wait(100);
    })
    .then(function () {
      var un = window.__calls.filter(function (c) { return c[0] === 'archive_session'; }).map(function (c) { return c[1]; });
      check('Undo unarchives it', un.length === 1 && un[0].key === picked[0].key && un[0].archived === false, JSON.stringify(un));
      // The one the engine holds: a command it left running.
      doneKey();
      return wait(300);
    })
    .then(function () {
      var a = asked()[1];
      check('the next is asked about too', !!a && a.key === picked[next].key && a.stop === false, JSON.stringify(a));
      check('held, it says why and how to end that too', /left `npm run dev` running in the background\. Mark it done again to end that too\./.test(toast()), toast());
      check('and comes back to the list', listed(next));
      // Asked again from its row, it ends that too.
      row(next).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Done ' + 'ABC'[next]).then(function () { return wait(50); });
    })
    .then(function () {
      doneKey();
      return wait(300);
    })
    .then(function () {
      var a = asked()[2];
      check('again, it ends what was running too', !!a && a.key === picked[next].key && a.stop === true, JSON.stringify(a));
      check('and it leaves the list', !listed(next));
      // The one that's working: held here, asking nothing.
      row(3).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Done W').then(function () { return wait(50); });
    })
    .then(function () {
      var n = asked().length;
      var b = hotkey('Done');
      check('the strip has Done, with its key', !!b && !b.disabled && /⇧⌘E/.test(b.textContent), b ? b.textContent : 'no button');
      check('working, its tooltip says the terminal only closes and the worktree stays', !!b && /its terminal just closes and the worktree .* made for it \(brave-otter\) stays/.test(b.title), b ? b.title : '');
      b.click();
      return wait(200).then(function () { return n; });
    })
    .then(function (n) {
      check('Done on one that’s working asks nothing yet', asked().length === n, JSON.stringify(asked()));
      check('and says it’s still working', /“Done W” is still working\. Mark it done again to end that too\./.test(toast()), toast());
      check('and it stays', title() === 'Done W' && listed(3), title());
      doneKey();
      return wait(300).then(function () { return n; });
    })
    .then(function (n) {
      var a = asked()[n];
      check('again, it stops it and is done with it', !!a && a.key === picked[3].key && a.stop === true, JSON.stringify(a));
      check('and it leaves the list', !listed(3));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
