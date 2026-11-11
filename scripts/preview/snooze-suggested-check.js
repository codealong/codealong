// Checks the snooze offered for a session whose last reply says its work goes on at a later time
// (snooze.ts `suggestedSnooze`, ui/SnoozeSuggestion.tsx): a line above its box says the time and
// who read the reply that way, and always shows its key; the session's Snooze menu has the time
// first, apart from the rest; ⌃⌘8 snoozes it until then from the list, and a click on the line
// does; a session whose reply names no time has no line, and ⌃⌘8 does nothing for it; and once
// snoozed, the offer is gone.
//   scripts/preview.sh sessions '' scripts/preview/snooze-suggested-check.js
(function () {
  // This runs before the app's script: the board it gets has three turns that ended, made from
  // real sessions on this device that aren't pinned (which list apart). Two wait for a later
  // time, one read by Jev and one tagged by an agent, and one names none.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var later = now + 3 * 86400e3;
  var plan = [
    { title: 'Waits A', waited: 30, insight: { landing: 'pushed', asksYou: 0.05, ending: 'report', waitsUntil: later } },
    { title: 'Waits B', waited: 20, insight: { landing: 'none', asksYou: 0.05, ending: 'report' } },
    { title: 'Waits C', waited: 10, insight: { asksYou: 0.05, ending: 'report', waitsUntil: later + 3600e3, by: 'claude:claude:boss', byTitle: 'Orchestrator', note: 'The release is on that day' } },
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
          if (s.insight) s.insight.waitsUntil = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = 'unread';
          s.statusSince = now - p.waited * 60e3;
          s.lastActivity = now - p.waited * 60e3;
          s.lastPromptAt = now - (p.waited + 5) * 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.quiet = false;
          s.helper = undefined;
          s.headless = false;
          s.agentSeen = undefined;
          // A side conversation's answer would take its box, which then offers nothing of the
          // session's own.
          s.sideAnswer = undefined;
          s.aside = undefined;
          s.connectionLost = undefined;
          s.limit = undefined;
          s.cutOff = undefined;
          s.background = undefined;
          s.origin = undefined;
          s.finished = undefined;
          s.suggestions = undefined;
          s.title = p.title;
          s.insight = p.insight;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var line = function () { return document.querySelector('.ca-sessinsp .ca-snoozesuggest .ca-suggest-reply'); };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title; }, 3000, plan[i].title + ' shown').then(function () { return wait(100); });
  };
  var chord = function (k, code) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: k, code: code, ctrlKey: true, metaKey: true })); };
  var texts = function (items) { return (items || []).map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var snoozes = function () { return sent('snooze_sessions'); };

  say('snooze suggested check: waiting for the list');
  until(function () { return picked.length === plan.length && [0, 1, 2].every(row); }, 20000, 'three unpinned sessions of this Mac to stand in')
    .then(function () { return show(0); })
    .then(function () {
      var l = line();
      check('a session whose reply waits for a later time is offered a snooze until then, above its box', !!l && /^Snooze until \S/.test(l.textContent) && !!l.closest('.ca-composer'), l ? l.textContent : 'no line');
      check('which says why', !!l && /as its reply says$/.test(l.textContent.replace(/[⌃⌘8\s]+$/, '')), l ? l.textContent : '');
      check('and who read it that way, under the pointer', !!l && /\(⌃⌘8\)\. Jev’s read$/.test(l.title), l ? l.title : '');
      var keys = l && l.querySelector('.ca-kbds');
      check('its key shows wherever the keyboard is', !!keys && getComputedStyle(keys).display !== 'none' && keys.textContent.replace(/\s/g, '') === '⌃⌘8', keys ? getComputedStyle(keys).display + ' ' + keys.textContent : 'no keys');
      return menuOf(row(0));
    })
    .then(function () {
      var group = lastMenu().filter(function (i) { return i.text === 'Snooze' && i.items; })[0];
      var items = texts(group && group.items);
      check('its Snooze menu has the time first, apart from the rest', /^Until .+, as its reply says$/.test(items[0] || '') && items[1] === '-' && items[2] === 'For 15 minutes', items.join(' | '));
      return show(1);
    })
    .then(function () {
      check('a session whose reply names no time is offered none', !line(), line() ? line().textContent : '');
      var before = snoozes().length;
      chord('8', 'Digit8');
      return wait(150).then(function () {
        check('and ⌃⌘8 does nothing for it', snoozes().length === before, JSON.stringify(snoozes()));
        return show(2);
      });
    })
    .then(function () {
      var l = line();
      check('an agent’s tag is said as its own, with its note', !!l && /Tagged by Orchestrator: The release is on that day$/.test(l.title), l ? l.title : 'no line');
      var before = snoozes().length;
      chord('8', 'Digit8');
      return until(function () { return snoozes().length > before; }, 3000, 'a snooze asked for by ⌃⌘8').then(function () { return wait(150); });
    })
    .then(function () {
      var s = snoozes().pop();
      check('⌃⌘8 snoozes it until the time its reply says', !!s && s.keys.join() === picked[2].key && s.until === plan[2].insight.waitsUntil, JSON.stringify(s));
      return show(0);
    })
    .then(function () {
      var before = snoozes().length;
      line().click();
      return until(function () { return snoozes().length > before; }, 3000, 'a snooze asked for by the click').then(function () { return wait(150); });
    })
    .then(function () {
      var s = snoozes().pop();
      check('a click on the line snoozes it until then too', !!s && s.keys.join() === picked[0].key && s.until === later, JSON.stringify(s));
      // Snoozed, it stays selected where the list keeps it, and nothing more is offered for it.
      var shown = title() === plan[0].title;
      check('and once it’s snoozed the offer is gone', !shown || !line(), line() ? line().textContent : '');
    })
    .then(done)
    .catch(stopped);
})();
