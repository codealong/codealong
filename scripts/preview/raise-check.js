// Checks Move to top (menus.ts `sessionActions`, ahead.ts, queue.ts `byPlace` and `leftAfter`,
// board.rs `apply_raise`): a session's menu has Move to top right before Snooze; moving one up
// asks the engine to, shows its Top chip at once and puts it first under its heading; ⌘] from the
// session shown then goes to it, ahead of what needs you and of one an agent moved up earlier,
// since it was moved up while you were elsewhere; its header says it's at the top. One an agent
// moved up says who and why in its chip's tooltip and header, and Back in line, in its menu, puts
// it back, its chip gone at once.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/raise-check.js
(function () {
  // This runs before the app's script: the board it gets has two that need you, one an agent
  // moved to the top and a turn that ended, made from real sessions on this device that aren't
  // pinned (which list apart), and nothing else waiting.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 30 * 60e3, title: 'Raise A' },
    { status: 'needs', since: now - 25 * 60e3, title: 'Raise B' },
    { status: 'unread', since: now - 20 * 60e3, title: 'Raise C', raised: { at: now - 60e3, agent: true, byTitle: 'Orchestrator', note: 'The deploy waits on it' } },
    { status: 'unread', since: now - 10 * 60e3, title: 'Raise D' },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      var pinned = board.pinned || [];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = p.status === 'needs';
          s.stalledSince = null;
          s.helper = undefined;
          s.title = p.title;
          s.raised = p.raised;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var status = function () { var t = document.querySelector('.ca-sessinsp-head'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chip = function (i) { var r = row(i); return r ? r.querySelector('.ca-raisedchip') : null; };
  var above = function (i, j) { return !!row(i) && !!row(j) && !!(row(i).compareDocumentPosition(row(j)) & Node.DOCUMENT_POSITION_FOLLOWING); };
  var asked = function () { return calls('raise_sessions').map(function (c) { return c[1]; }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var next = function (name) {
    key(window, { key: ']', code: 'BracketRight', metaKey: true });
    return on(name);
  };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return on(plan[i].title).then(function () { return wait(50); });
  };

  say('raise check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000, 'four unpinned sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      check('one an agent moved up has the Top chip', !!chip(2));
      check('which says who moved it and why', !!chip(2) && /^Moved to the top by Orchestrator: The deploy waits on it\./.test(chip(2).title), chip(2) ? chip(2).title : '');
      check('and leads its heading', above(2, 3));
      return show(0);
    })
    .then(function () { return menuOf(row(3)); })
    .then(function (labels) {
      var at = labels.indexOf('Move to top');
      check('a session’s menu has Move to top', at >= 0, labels.join(' | '));
      check('right before Snooze', at >= 0 && /^Snooze › /.test(labels[at + 1]), labels.slice(at, at + 2).join(' | '));
      check('its row has no Top chip', !chip(3));
      menuItem('Move to top')();
      return until(function () { return asked().length === 1; }, 3000, 'Move to top asking the engine').then(function () { return wait(100); });
    })
    .then(function () {
      var a = asked()[0];
      check('Move to top asks the engine to move it up', !!a && a.keys.length === 1 && a.keys[0] === picked[3].key && a.raised === true, JSON.stringify(a));
      check('its row has the Top chip at once', !!chip(3));
      check('and it’s first under its heading, ahead of the agent’s', above(3, 2));
      check('the window stays on the session shown', title() === 'Raise A', title());
      return next('Raise D');
    })
    .then(function () {
      check('⌘] goes to it next, ahead of what needs you', title() === 'Raise D', title());
      check('its header says it’s at the top', /moved to the top(?! by)/.test(status()), status());
      return next('Raise C');
    })
    .then(function () {
      check('⌘] from it goes on in the queue’s order: the agent’s next', title() === 'Raise C', title());
      check('whose header says the agent moved it', /moved to the top by Orchestrator/.test(status()), status());
      return next('Raise A');
    })
    .then(function () {
      check('then what needs you', title() === 'Raise A', title());
      return menuOf(row(2));
    })
    .then(function (labels) {
      check('the agent’s menu says Back in line', labels.indexOf('Back in line') >= 0 && labels.indexOf('Move to top') < 0, labels.join(' | '));
      menuItem('Back in line')();
      return until(function () { return asked().length === 2; }, 3000, 'Back in line asking the engine').then(function () { return wait(100); });
    })
    .then(function () {
      var a = asked()[1];
      check('Back in line asks the engine to put it back', !!a && a.keys[0] === picked[2].key && a.raised === false, JSON.stringify(a));
      check('and its chip goes at once', !chip(2));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
