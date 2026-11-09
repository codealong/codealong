// Checks the labels you put on sessions (state.rs `labels`, board.rs `apply_labels`, ahead.ts
// `labelAhead`): a session's row shows its labels as chips, and a double-click on one shows only
// the sessions with it, as the sidebar's Labels does, counting each; its header shows them too,
// each with a ✕ that takes it off, and a chip whose picker puts one on or makes a new one; and its
// menu has Labels ›, each checked while it wears it. Each change asks the engine by name and shows
// at once.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/session-labels-check.js
(function () {
  // This runs before the app's script: the board it gets has three sessions made from real ones
  // on this device, two of them labelled, and the labels todos wear.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var bug = { id: 'l-bug', name: 'Bug', color: 'red' };
  var release = { id: 'l-rel', name: 'Release', color: 'blue' };
  var ui = { id: 'l-ui', name: 'UI', color: 'purple' };
  var plan = [
    { title: 'Label A', labels: [bug, release], at: now - 5 * 60e3 },
    { title: 'Label B', labels: [bug], at: now - 10 * 60e3 },
    { title: 'Label C', labels: [], at: now - 15 * 60e3 },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.labels = [bug, release, ui];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          s.labels = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || s.helper || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.title = p.title;
          s.labels = p.labels.length ? p.labels : undefined;
          s.lastActivity = p.at;
          s.lastPromptAt = p.at;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chipsIn = function (el) { return el ? all('.ca-labelchip:not(.ca-labelchip--add)', el).map(function (c) { return c.textContent.trim(); }) : []; };
  var head = function () { return q('.ca-sessinsp-head'); };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var asked = function () { return calls('label_sessions').map(function (c) { return c[1]; }); };
  var sideChip = function (name) { return all('.ca-side-chips button').filter(function (b) { return b.textContent.indexOf(name) === 0; })[0]; };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title; }, 3000, 'its panel').then(function () { return wait(50); });
  };
  var twice = function (el) { el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true })); };

  say('session labels check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(2); }, 20000, 'three sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      check('a row shows its labels, in the order they were put on', chipsIn(row(0)).join(',') === 'Bug,Release', chipsIn(row(0)).join(','));
      check('one without labels shows none', chipsIn(row(2)).length === 0, chipsIn(row(2)).join(','));
      var b = sideChip('Bug');
      var r = sideChip('Release');
      check('the sidebar’s Labels counts the sessions with each', !!b && /Bug\s*2$/.test(b.textContent) && !!r && /Release\s*1$/.test(r.textContent), (b ? b.textContent : '-') + ' / ' + (r ? r.textContent : '-'));
      twice(row(0).querySelectorAll('.ca-labelchip')[1]);
      return until(function () { return !row(1); }, 3000, 'Release to narrow the list');
    })
    .then(function () {
      check('a double-click on a row’s label shows only the sessions with it', !!row(0) && !row(1) && !row(2));
      var r = sideChip('Release');
      check('and picks it in the sidebar', !!r && r.getAttribute('aria-pressed') === 'true', r ? r.outerHTML.slice(0, 120) : '');
      twice(row(0).querySelectorAll('.ca-labelchip')[1]);
      return until(function () { return row(1) && row(2); }, 3000, 'a second double-click to show them all again');
    })
    .then(function () { return show(2); })
    .then(function () {
      var add = q('.ca-labelchip--add', head());
      check('a header without labels offers Add label', !!add && /Add label/.test(add.textContent), add ? add.textContent : 'none');
      add.click();
      return until(function () { return q('.ca-labelpick'); }, 2000, 'its picker');
    })
    .then(function () {
      var items = all('.ca-labelpick .ca-menu-item').map(function (b) { return b.textContent.trim(); });
      check('its picker lists the labels todos wear', items.join(',') === 'Bug,Release,UI', items.join(','));
      all('.ca-labelpick .ca-menu-item').filter(function (b) { return b.textContent.trim() === 'UI'; })[0].click();
      return until(function () { return asked().length === 1; }, 3000, 'picking one asking the engine');
    })
    .then(function () {
      var a = asked()[0];
      check('picking one asks the engine to put it on, by name', !!a && a.keys.join() === picked[2].key && a.add.join() === 'UI' && a.remove.length === 0, JSON.stringify(a));
      check('the row shows it at once', chipsIn(row(2)).join(',') === 'UI', chipsIn(row(2)).join(','));
      type(q('.ca-labelpick-input'), 'Spike');
      key(q('.ca-labelpick-input'), { key: 'Enter' });
      return until(function () { return asked().length === 2; }, 3000, 'a new name asking the engine');
    })
    .then(function () {
      var a = asked()[1];
      check('a new name typed asks the engine to make it and put it on', !!a && a.add.join() === 'Spike', JSON.stringify(a));
      check('and shows at once, in the next color not taken', chipsIn(row(2)).join(',') === 'UI,Spike' && !!q('.ca-labeldot.is-pink', row(2)), chipsIn(row(2)).join(','));
      key(q('.ca-labelpick-input'), { key: 'Escape' });
      return show(0);
    })
    .then(function () {
      check('its header shows its labels', chipsIn(head()).join(',') === 'Bug,Release', chipsIn(head()).join(','));
      var x = q('[aria-label="Remove Release"]', head());
      check('each with a ✕', !!x);
      x.click();
      return until(function () { return asked().length === 3; }, 3000, 'the ✕ asking the engine');
    })
    .then(function () {
      var a = asked()[2];
      check('the ✕ asks the engine to take it off, by name', !!a && a.keys.join() === picked[0].key && a.remove.join() === 'Release' && a.add.length === 0, JSON.stringify(a));
      check('and it goes from the row at once', chipsIn(row(0)).join(',') === 'Bug', chipsIn(row(0)).join(','));
      return menuOf(row(1));
    })
    .then(function (labels) {
      var ours = labels.filter(function (l) { return /^Labels › /.test(l); });
      check('a session’s menu has Labels ›, each checked while it wears it', ours.join(' | ') === 'Labels › Bug ✓ | Labels › Release | Labels › UI', ours.join(' | '));
      menuItem('Labels › Bug ✓')();
      return until(function () { return asked().length === 4; }, 3000, 'the menu asking the engine');
    })
    .then(function () {
      var a = asked()[3];
      check('a checked one takes it off', !!a && a.keys.join() === picked[1].key && a.remove.join() === 'Bug', JSON.stringify(a));
      check('at once', chipsIn(row(1)).length === 0, chipsIn(row(1)).join(','));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
