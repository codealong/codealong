// Checks where the quick panel starts a session in a repo whose main checkout holds work under way
// (quick.ts `startLaneIn`, start.rs `note_isolation`): on a place that moves the session into a
// worktree of its own before it changes anything (a Claude Code account's Background), the repo
// picked starts in its main checkout, since the session joins none of that work; on one that
// doesn't (the terminal), where new work goes, a reusable worktree ready for it. The repo picked
// follows the place picked after it, both ways.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-isolated-check.js
// The board needs a repo whose main checkout holds work beside a reusable worktree ready for new
// work, and an account whose Background isolates there and that starts in a terminal too.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chipFor = function (keys) { return function () { return all('.ca-quick-chip').filter(function (b) { return b.title.indexOf(keys) >= 0; })[0]; }; };
  var repoChip = chipFor('Repo (⌘T)');
  var treeChip = chipFor('Worktree (⌘B)');
  var said = function (chip) { var c = chip(); return c ? c.querySelector('span').textContent : null; };
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var placeNow = function () { var p = q('.ca-quick-agent .ca-quick-chip-place'); return p ? p.textContent.trim() : ''; };
  var placeRow = function () { return all('.ca-quick-agent .ca-picker-foot-row').filter(function (r) { var l = q('.ca-picker-foot-label', r); return l && l.textContent === 'Runs in'; })[0]; };
  var filter = function () { return q('.ca-picker-filter'); };
  var options = function () { return all('.ca-picker-opt'); };
  var titles = function () { return options().map(function (o) { return o.querySelector('.ca-picker-title').textContent; }); };
  var startTitle = function () { var r = all('.ca-quick-row-title').filter(function (t) { return /^Start /.test(t.textContent); }); return r.length ? r[r.length - 1].textContent : null; };
  var press = function (letter) { key(input(), { key: letter, code: 'Key' + letter.toUpperCase(), metaKey: true }); };
  var closed = function () { return until(function () { return !filter(); }, 3000, 'the menu closed'); };
  var found = null;

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
      if (!b) throw new Error('no ' + label + ' in Runs in: ' + all('button', placeRow()).map(function (o) { return o.textContent.trim(); }).join(', '));
      b.click();
      return until(function () { return placeNow() === label; }, 2000, 'the chip doesn’t say ' + label);
    }).then(function () {
      key(filter(), { key: 'Escape' });
      return closed();
    });
  };

  say('quick isolated check: waiting for the panel');
  until(function () { return !!input() && !!repoChip() && !repoChip().disabled && calls('quick_ready').length > 0; }, 20000, 'the repo chip')
    .then(function () { return Promise.all([window.__TAURI_INTERNALS__.invoke('get_board'), window.__TAURI_INTERNALS__.invoke('start_options')]); })
    .then(function (got) {
      var lanes = got[0].lanes.filter(function (l) { return !l.startsChats && !l.hidden && !l.device; });
      var of = function (m) { return lanes.filter(function (l) { return l.project === m.project && l.repo === m.repo; }); };
      var ready = function (m) { return of(m).filter(function (l) { return l.origin !== 'main' && (l.reuse === 'free' || l.reuse === 'landed'); }); };
      got[1].accounts.forEach(function (a) {
        if (found) return;
        var isolating = a.places.find(function (p) { return (p.isolatesIn || []).length > 0; });
        var plain = a.places.find(function (p) { return p.place === 'terminal'; });
        if (!isolating || !plain) return;
        var main = lanes.find(function (l) {
          return l.origin === 'main' && (l.reuse === 'open' || l.reuse === 'review') && isolating.isolatesIn.indexOf(l.path) >= 0 && (plain.isolatesIn || []).indexOf(l.path) < 0 && ready(l).length > 0;
        });
        if (main) found = { account: a.name, isolating: isolating.label, plain: plain.label, main: main, ready: ready(main).map(function (l) { return l.callsign; }) };
      });
      if (!found) throw new Error('no repo whose main checkout holds work beside a reusable worktree ready for new work, with an account whose Background isolates there and that starts in a terminal too');
      say('quick isolated check: ' + found.main.repo + ' on ' + found.account + ', ready: ' + found.ready.join(', '));
      return pickAccount(found.account);
    })
    .then(function () { return pickPlace(found.isolating); })
    .then(function () {
      press('t');
      return until(function () { return options().length > 0; }, 3000, 'the repo menu');
    })
    .then(function () {
      type(filter(), found.main.repo);
      return wait(100);
    })
    .then(function () {
      var row = options().filter(function (o) { var t = o.querySelector('.ca-picker-title').textContent; return t === found.main.repo || t === found.main.project + ' · ' + found.main.repo; })[0];
      if (!row) throw new Error('no row for ' + found.main.repo + ': ' + titles().join(' | '));
      var detail = row.querySelector('.ca-picker-detail');
      check('the repo menu says it starts in main on ' + found.isolating, !!detail && /^main/.test(detail.textContent), detail ? detail.textContent : '(no detail)');
      row.click();
      return closed();
    })
    .then(function () {
      check('on ' + found.isolating + ' the repo picked starts in main, work under way there or not', said(repoChip) === found.main.repo && said(treeChip) === 'main', said(repoChip) + ' / ' + said(treeChip));
      check('and the start row names the checkout', startTitle() === 'Start in ' + found.main.callsign + ' (main)', startTitle());
      return pickPlace(found.plain);
    })
    .then(function () {
      check('in the ' + found.plain + ' it follows to a worktree ready for new work', found.ready.indexOf(said(treeChip)) >= 0, said(treeChip) + ' (ready: ' + found.ready.join(', ') + ')');
      return pickPlace(found.isolating);
    })
    .then(function () {
      check('and back to main on ' + found.isolating, said(treeChip) === 'main', said(treeChip));
      done();
    })
    .catch(stopped);
})();
