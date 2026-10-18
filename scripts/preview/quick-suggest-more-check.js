// Checks what Suggested lists in the quick panel (Quick.tsx, route.ts `suggestions`): up to five
// rows, the rows left after the likeliest going to more of the sessions Jev weighed; a start Jev
// suggests that would start just what the start row does left out, the start row saying how
// likely Jev thinks it is and ⏎ going to it when that start led; a start the start row wouldn't
// make (a new worktree) kept; and Send to another session… under the suggestions, with ⌘F, the
// rows below it keeping ⌘ digits in order.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-suggest-more-check.js
// The board needs a worktree that isn't hidden and five sessions in such worktrees.
(function () {
  // This runs before the app's script: one worktree is pinned, so the start row starts there,
  // and takes new work; five sessions get titles nothing else has. Jev is on, answering that
  // what's typed is new work in that worktree (in a worktree of its own when the prompt says so),
  // with those five sessions weighed.
  localStorage.setItem('codealong.quick.project', 'All projects');
  // Where starts last opened, which a choice without a `start` setting would follow.
  localStorage.removeItem('codealong.start');
  var picked = { lane: null, sessions: [] };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    if (cmd === 'route_quick') {
      var own = /of its own/.test((args || {}).text || '');
      return Promise.resolve({
        action: 'start',
        actions: [['start', 0.8], ['send', 0.2]],
        sessions: picked.sessions.map(function (key, i) { return { id: key, p: [0.3, 0.2, 0.15, 0.1, 0.08][i] }; }),
        noSession: 0.2,
        lanes: [{ id: picked.lane, p: 0.9 }],
        repos: [],
        ownWorktree: own ? 0.9 : 0,
        ownWorktreeAsked: own ? 0.9 : 0,
        habit: { new: 0, of: 0 },
        duplicate: null,
        model: 'jev-1.14.0',
        ms: 200,
        inputTokens: 9000,
      });
    }
    var p = real(cmd, args, options);
    // Every account starts in a terminal and makes worktrees, so a start in a new worktree is a
    // start of its own whatever this Mac's settings say.
    if (cmd === 'start_options') {
      return p.then(function (o) {
        (o.accounts || []).forEach(function (a) {
          a.worktree = true;
          a.chosen = 'terminal';
          a.places = [{ place: 'terminal', label: 'Terminal' }].concat((a.places || []).filter(function (x) { return x.place !== 'terminal'; }));
        });
        return o;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.jev = Object.assign({}, board.jev, { quick_panel: true });
      board.todos = [];
      board.prs = [];
      var n = 0;
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
          if (i < 0 && picked.sessions.length >= 5) return;
          if (i < 0) i = picked.sessions.push(s.key) - 1;
          s.title = 'Jev pick ' + (i + 1);
          s.status = 'idle';
          s.live = true;
          delete s.snoozedUntil;
          n++;
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var group = function (name) { return Array.prototype.filter.call(document.querySelectorAll('.ca-quick-group'), function (g) { return g.getAttribute('aria-label') === name; })[0]; };
  var rowsOf = function (g) { return g ? Array.prototype.slice.call(g.querySelectorAll('.ca-quick-row')) : []; };
  var compose = function () { return document.querySelector('.ca-quick-compose'); };
  var keyOf = function (r) { var k = r.querySelector('.ca-kbds'); return k ? k.textContent.replace(/\s/g, '') : ''; };
  var ask = function (text) {
    type(input(), '');
    type(input(), text);
    return until(function () { var g = group('Suggested'); return !!g && rowsOf(g).some(function (r) { return /Jev pick 1/.test(r.textContent); }); }, 5000, 'Jev’s suggestions');
  };

  say('quick suggest more check: waiting for the list');
  until(function () { return picked.sessions.length === 5 && !!input() && document.querySelectorAll('.ca-quick-row').length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () { return ask('Please tidy the release notes before Friday'); })
    .then(function () {
      var rows = rowsOf(group('Suggested'));
      var sends = rows.filter(function (r) { return /Send to “Jev pick/.test(r.textContent); });
      check('more of the sessions Jev weighed are suggested, in its order', sends.length === 4 && sends.every(function (r, i) { return r.textContent.indexOf('Jev pick ' + (i + 1)) >= 0; }), rows.map(function (r) { return r.textContent; }).join(' | '));
      check('the start the start row makes is left out', !rows.some(function (r) { return /^Start in/.test(r.querySelector('.ca-quick-row-title').textContent); }));
      check('the start row says how likely Jev thinks it is', /72%/.test(compose().textContent), compose().textContent);
      check('⏎ goes to the start row, the start that led', compose().classList.contains('is-active'));
      var last = rows[rows.length - 1];
      check('Send to another session… comes under the suggestions, with ⌘F', /Send to another session/.test(last.textContent) && keyOf(last) === '⌘F', last.textContent);
      check('the suggestions take ⌘1 to ⌘4', sends.map(keyOf).join(' ') === '⌘1 ⌘2 ⌘3 ⌘4', sends.map(keyOf).join(' '));
      var heading = group('Suggested').querySelector('.ca-quick-glabel').textContent;
      check('Suggested counts its suggestions', /4/.test(heading) && !/5/.test(heading), heading);
      var before = sent('start_from_quick').length;
      key(window, { key: '1', code: 'Digit1', metaKey: true });
      return wait(300).then(function () {
        var s = sent('send_to_session').pop();
        check('⌘1 does the first suggestion', !!s && s.key === picked.sessions[0] && sent('start_from_quick').length === before, JSON.stringify(s));
      });
    })
    .then(function () { return ask('Please tidy the release notes in a worktree of its own'); })
    .then(function () {
      var rows = rowsOf(group('Suggested'));
      var first = rows[0];
      check('a start the start row wouldn’t make is kept, and leads', !!first && /^Start in a new worktree/.test(first.querySelector('.ca-quick-row-title').textContent) && first.classList.contains('is-active'), first && first.textContent);
      check('the start row then says no likelihood of its own', !/%/.test(compose().querySelector('.ca-quick-sure').textContent), compose().textContent);
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
