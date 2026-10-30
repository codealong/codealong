// Checks a turn its own agent marked seen (board.rs `apply_agent_seen`, LaneRow.tsx
// `AgentSeenChip`, ahead.ts): its row has the Seen by agent chip, whose tooltip has the agent's
// note, and a row without the mark has none; its header says seen by its agent; and Mark as unread,
// in its menu, asks the engine and makes it Your turn at once, its chip gone.
//   scripts/preview.sh sessions '' scripts/preview/agent-seen-check.js
(function () {
  // This runs before the app's script: the board it gets has two turns that ended and were seen,
  // one of them by its agent, made from real sessions on this device that aren't pinned (which
  // list apart).
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { title: 'Agent seen A', agentSeen: { at: now - 12 * 60e3, note: 'Nothing new on the deploy.' } },
    { title: 'Agent seen B' },
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
          s.agentSeen = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = 'idle';
          s.statusSince = now - 10 * 60e3;
          s.lastActivity = now - 10 * 60e3;
          s.lastPromptAt = now - 15 * 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.quiet = false;
          s.helper = undefined;
          s.title = p.title;
          s.agentSeen = p.agentSeen;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var status = function () { var t = document.querySelector('.ca-sessinsp-head'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chip = function (i) { var r = row(i); return r ? r.querySelector('.ca-agentseenchip') : null; };

  say('agent seen check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(1); }, 20000, 'two unpinned sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      check('a turn its agent marked seen has the chip', !!chip(0) && chip(0).textContent === 'Seen by agent', chip(0) ? chip(0).textContent : '');
      check('whose tooltip has the agent’s note', !!chip(0) && /^Seen by its agent, which said this turn needs nothing from you: Nothing new on the deploy\. Mark as unread/.test(chip(0).title), chip(0) ? chip(0).title : '');
      check('one its agent didn’t mark has none', !chip(1));
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Agent seen A'; }, 3000, 'the session shown').then(function () { return wait(50); });
    })
    .then(function () {
      check('its header says seen by its agent', / · seen by its agent/.test(status()), status());
      return menuOf(row(0));
    })
    .then(function (labels) {
      check('its menu has Mark as unread', labels.indexOf('Mark as unread') >= 0, labels.join(' | '));
      menuItem('Mark as unread')();
      return until(function () { return calls('mark_unread').length === 1; }, 3000, 'Mark as unread asking the engine').then(function () { return wait(100); });
    })
    .then(function () {
      var a = calls('mark_unread')[0][1];
      check('Mark as unread asks the engine', !!a && a.keys.length === 1 && a.keys[0] === picked[0].key, JSON.stringify(a));
      check('and its chip goes at once', !chip(0));
      check('its header no longer says its agent saw it', !/seen by its agent/.test(status()), status());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
