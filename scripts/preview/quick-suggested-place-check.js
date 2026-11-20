// Checks where a start Jev suggests on another agent than the start row's goes in a repo whose
// main checkout holds work under way (route.ts `suggestions`, Quick.tsx `isolatedFor`): by that
// agent's own place, not the start row's. With the start row on a place that moves a session out
// of the main checkout (a Claude Code account's Background), the start row starts there; Jev's
// start on an account whose places don't goes where new work goes in the repo, a reusable worktree
// ready for it, and ⌘1 starts it there.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-suggested-place-check.js
// The board needs a repo whose main checkout holds work beside a reusable worktree ready for new
// work, an account whose Background isolates there, and an account of another harness whose
// places don't.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.start');
  var found = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    if (cmd === 'route_quick') {
      // New work in the repo, on the other account, Jev sure of its main checkout.
      return Promise.resolve({
        action: 'start',
        actions: [['start', 1]],
        sessions: [],
        noSession: 1,
        lanes: found ? [{ id: found.main.id, p: 0.9 }] : [],
        repos: found ? [{ id: found.main.project + '/' + found.main.repo, p: 0.9 }] : [],
        ownWorktree: 0,
        ownWorktreeAsked: 0,
        habit: { new: 0, of: 0 },
        duplicate: null,
        agents: found ? [{ account: found.other, model: null, p: 0.9 }] : [],
        model: 'jev-1.14.0',
        ms: 200,
        inputTokens: 9000,
      });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.decisions = Object.assign({}, board.decisions, { quick_panel: true });
      return board;
    });
  };

  var input = function () { return q('.ca-quick-input textarea'); };
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var placeNow = function () { var p = q('.ca-quick-agent .ca-quick-chip-place'); return p ? p.textContent.trim() : ''; };
  var placeRow = function () { return all('.ca-quick-agent .ca-picker-foot-row').filter(function (r) { var l = q('.ca-picker-foot-label', r); return l && l.textContent === 'Runs in'; })[0]; };
  var filter = function () { return q('.ca-picker-filter'); };
  var group = function (name) { return all('.ca-quick-group').filter(function (g) { return g.getAttribute('aria-label') === name; })[0]; };
  var titleOf = function (r) { var t = q('.ca-quick-row-title', r); return t ? t.textContent : ''; };
  var compose = function () { return q('.ca-quick-compose'); };

  // Picks an account on its own default model in the agent menu, by the section its name heads.
  var pickAccount = function (name) {
    key(window, { key: 'l', metaKey: true });
    return until(function () { return all('.ca-picker-section').length > 0; }, 3000).then(function () {
      var section = all('.ca-picker-section').find(function (s) { var n = q('.ca-quick-acct-name', s); return !!n && n.textContent === name; });
      var opt = section && q('.ca-picker-opt', section);
      if (!opt) throw new Error('no option for ' + name + ' in the agent menu');
      opt.click();
      return until(function () { return agentChip().textContent.indexOf(name) >= 0; }, 3000);
    });
  };

  // Picks a place by its label in the agent menu's Runs in row (⌘O).
  var pickPlace = function (label) {
    key(window, { key: 'o', metaKey: true });
    return until(function () { return placeRow(); }, 3000, 'no Runs in row on ⌘O').then(function () {
      var b = all('button', placeRow()).find(function (o) { return o.textContent.trim() === label; });
      if (!b) throw new Error('no ' + label + ' in Runs in');
      b.click();
      return until(function () { return placeNow() === label; }, 2000, 'the chip doesn’t say ' + label);
    }).then(function () {
      key(filter(), { key: 'Escape' });
      return until(function () { return !filter(); }, 3000, 'the menu closed');
    });
  };

  say('quick suggested place check: waiting for the panel');
  until(function () { return !!input() && calls('quick_ready').length > 0; }, 20000, 'the panel')
    .then(function () { return Promise.all([window.__TAURI_INTERNALS__.invoke('get_board'), window.__TAURI_INTERNALS__.invoke('start_options')]); })
    .then(function (got) {
      var lanes = got[0].lanes.filter(function (l) { return !l.startsChats && !l.hidden && !l.device; });
      var accounts = got[1].accounts.filter(function (a) { return (a.places || []).length > 0; });
      var ready = function (m) { return lanes.filter(function (l) { return l.project === m.project && l.repo === m.repo && l.origin !== 'main' && (l.reuse === 'free' || l.reuse === 'landed'); }); };
      var isolates = function (a, path) { return a.places.some(function (p) { return (p.isolatesIn || []).indexOf(path) >= 0; }); };
      accounts.forEach(function (a) {
        if (found) return;
        var isolating = a.places.find(function (p) { return (p.isolatesIn || []).length > 0; });
        if (!isolating) return;
        var main = lanes.find(function (l) { return l.origin === 'main' && (l.reuse === 'open' || l.reuse === 'review') && isolating.isolatesIn.indexOf(l.path) >= 0 && ready(l).length > 0; });
        if (!main) return;
        // An account named resolves to the one of its harness with room, so no account of the
        // other's harness may isolate there.
        var other = accounts.find(function (b) {
          return b.harness !== a.harness && !accounts.some(function (c) { return c.harness === b.harness && isolates(c, main.path); });
        });
        if (other) found = { account: a.name, isolating: isolating.label, other: other.name, main: main, ready: ready(main).map(function (l) { return l.id; }) };
      });
      if (!found) throw new Error('no repo whose main checkout holds work beside a reusable worktree ready for new work, with an account whose Background isolates there and one of another harness whose places don’t');
      say('quick suggested place check: ' + found.main.repo + ', start row on ' + found.account + ', Jev’s start on ' + found.other);
      return pickAccount(found.account);
    })
    .then(function () { return pickPlace(found.isolating); })
    .then(function () {
      // The repo, picked in its menu (⌘T), so the start row starts in it.
      key(input(), { key: 't', code: 'KeyT', metaKey: true });
      return until(function () { return all('.ca-picker-opt').length > 0; }, 3000, 'the repo menu');
    })
    .then(function () {
      type(filter(), found.main.repo);
      return wait(100);
    })
    .then(function () {
      var row = all('.ca-picker-opt').filter(function (o) { var t = o.querySelector('.ca-picker-title').textContent; return t === found.main.repo || t === found.main.project + ' · ' + found.main.repo; })[0];
      if (!row) throw new Error('no row for ' + found.main.repo + ' in the repo menu');
      row.click();
      return until(function () { return !filter(); }, 3000, 'the menu closed');
    })
    .then(function () {
      type(input(), 'Please tidy the release notes before Friday');
      return until(function () { var g = group('Suggested'); return !!g && all('.ca-quick-row', g).some(function (r) { return /^Start in/.test(titleOf(r)); }); }, 5000, 'Jev’s start');
    })
    .then(function () {
      var row = all('.ca-quick-row', group('Suggested')).filter(function (r) { return /^Start in/.test(titleOf(r)); })[0];
      var startRow = titleOf(compose());
      check('the start row starts in ' + found.main.repo + '’s main checkout, on ' + found.isolating, startRow === 'Start in ' + found.main.callsign + ' (main)', startRow);
      check('Jev’s start on ' + found.other + ' names a worktree ready for new work, not the main checkout', !/\(main\)/.test(titleOf(row)), titleOf(row));
      var before = sent('start_from_quick').length;
      key(window, { key: '1', code: 'Digit1', metaKey: true });
      return until(function () { return sent('start_from_quick').length > before; }, 3000, 'a start').then(function () {
        var spec = sent('start_from_quick').pop().spec;
        check('⌘1 starts it there, on ' + found.other, found.ready.indexOf(spec.laneId) >= 0 && spec.account !== found.account, spec.laneId + ' on ' + spec.account);
      });
    })
    .then(done)
    .catch(stopped);
})();
