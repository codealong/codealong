// Checks what the quick panel keeps of a route (Quick.tsx `noteDone`, routelog.rs `Note`): as
// you act on what you typed, the engine is told the text as it went to Jev, Jev's suggestions as
// listed, what you did and how you got to it. ⏎ on the first suggestion, ⌘2 on the second, a
// start from the new session's row (⌘⏎) and Send this prompt to it from a suggestion's menu are
// each noted; a search too short to go to Jev isn't.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-route-log-check.js
// The board needs a worktree that isn't hidden and two sessions in such worktrees.
(function () {
  // This runs before the app's script: one worktree is pinned, so the start row starts there;
  // two sessions get titles nothing else has. Jev is on, answering that what's typed goes to the
  // first of them, or else to the second.
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.start');
  var picked = { lane: null, sessions: [] };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    if (cmd === 'route_quick') {
      return Promise.resolve({
        action: 'send',
        actions: [['send', 0.9], ['start', 0.1]],
        sessions: [{ id: picked.sessions[0], p: 0.6 }, { id: picked.sessions[1], p: 0.3 }],
        noSession: 0.1,
        lanes: [{ id: picked.lane, p: 0.9 }],
        repos: [],
        ownWorktree: 0,
        ownWorktreeAsked: 0,
        habit: { new: 0, of: 0 },
        duplicate: null,
        model: 'jev-1.14.0',
        ms: 200,
        inputTokens: 9000,
      });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.jev = Object.assign({}, board.jev, { quick_panel: true });
      board.todos = [];
      board.prs = [];
      board.lanes.forEach(function (l) {
        if (l.hidden || l.device || l.startsChats) return;
        if (!picked.lane || picked.lane === l.id) {
          picked.lane = l.id;
          l.reuse = 'free';
          board.pinnedWorktrees = [l.id];
        }
        l.sessions.forEach(function (s) {
          if (s.visitor || s.archived || s.helper || s.headless) return;
          var i = picked.sessions.indexOf(s.key);
          if (i < 0 && picked.sessions.length >= 2) return;
          if (i < 0) i = picked.sessions.push(s.key) - 1;
          s.title = 'Logged pick ' + (i + 1);
          s.status = 'idle';
          s.live = true;
          delete s.snoozedUntil;
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var suggested = function () { return Array.prototype.filter.call(document.querySelectorAll('.ca-quick-group'), function (g) { return g.getAttribute('aria-label') === 'Suggested'; })[0]; };
  var notes = function () { return sent('note_route').map(function (a) { return a.note; }); };
  var ask = function (text) {
    type(input(), '');
    type(input(), text);
    return until(function () { var g = suggested(); return !!g && /Logged pick 2/.test(g.textContent) && !g.querySelector('.is-stale'); }, 5000, 'Jev’s suggestions');
  };
  var did = function (n) { return n && JSON.stringify([n.did.kind, n.did.target, n.via]); };

  say('quick route log check: waiting for the list');
  until(function () { return picked.sessions.length === 2 && !!input() && document.querySelectorAll('.ca-quick-row').length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () { return ask('Please tidy the release notes before Friday'); })
    .then(function () {
      key(input(), { key: 'Enter' });
      return until(function () { return notes().length === 1; }, 3000, 'the first note');
    })
    .then(function () {
      var n = notes()[0];
      check('⏎ notes the text as it went to Jev, in the panel’s project', n.project === 'All projects' && n.text === 'Please tidy the release notes before Friday', JSON.stringify(n));
      var listed = (n.suggested || []).map(function (c) { return c.kind + ' ' + c.target; });
      check('with Jev’s suggestions as listed, the likeliest first', listed[0] === 'send ' + picked.sessions[0] && listed[1] === 'send ' + picked.sessions[1], JSON.stringify(n.suggested));
      check('and what you did: sent it to the first, from a suggestion', did(n) === JSON.stringify(['send', picked.sessions[0], 'suggested']), did(n));
      return wait(300);
    })
    .then(function () { return ask('Please tidy the changelog before Monday'); })
    .then(function () {
      key(window, { key: '2', code: 'Digit2', metaKey: true });
      return until(function () { return notes().length === 2; }, 3000, 'the second note');
    })
    .then(function () {
      var n = notes()[1];
      check('⌘2 notes the second suggestion', did(n) === JSON.stringify(['send', picked.sessions[1], 'suggested']), did(n));
      return wait(300);
    })
    .then(function () { return ask('Please tidy the roadmap before Tuesday'); })
    .then(function () {
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return notes().length === 3; }, 3000, 'the third note');
    })
    .then(function () {
      var n = notes()[2];
      check('⌘⏎ notes a start from the new session’s row, in its worktree', did(n) === JSON.stringify(['start', picked.lane, 'start-row']), did(n));
      return wait(300);
    })
    .then(function () { return ask('Please tidy the backlog before Thursday'); })
    .then(function () {
      var row = Array.prototype.filter.call(suggested().querySelectorAll('.ca-quick-row'), function (r) { return /Logged pick 2/.test(r.textContent); })[0];
      return menuOf(row);
    })
    .then(function () {
      menuItem('Send this prompt to it')();
      return until(function () { return notes().length === 4; }, 3000, 'the menu’s note');
    })
    .then(function () {
      var n = notes()[3];
      check('a suggestion’s menu notes what it sends', did(n) === JSON.stringify(['send', picked.sessions[1], 'suggested']) && n.text === 'Please tidy the backlog before Thursday', did(n));
      return wait(300);
    })
    .then(function () {
      type(input(), '');
      type(input(), 'Logged');
      return until(function () { return Array.prototype.some.call(document.querySelectorAll('.ca-quick-row'), function (r) { return /Logged pick 1/.test(r.textContent); }); }, 5000, 'the search');
    })
    .then(function () {
      key(input(), { key: 'Enter' });
      return wait(500);
    })
    .then(function () {
      check('a search too short to go to Jev isn’t noted', notes().length === 4, JSON.stringify(notes().slice(4)));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
