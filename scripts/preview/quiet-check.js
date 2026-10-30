// Checks Quiet (menus.ts `sessionActions`, ahead.ts, board.rs `apply_quiet`): a session's menu
// has Quiet beside Snooze, checked while it's quiet; quieting one whose turn ended asks the engine
// to quiet it, shows it idle with the Quiet chip at once and goes on to the next that waits on
// you, the strip saying how the last one went; its header says it's quiet; and unchecking it on
// one that's quiet makes it heard again, its chip gone at once.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/quiet-check.js
(function () {
  // This runs before the app's script: the board it gets has two turns that ended, then one
  // that's quiet already, made from real sessions on this device, and nothing else waiting.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'unread', since: now - 20 * 60e3, quiet: false, title: 'Quiet A' },
    { status: 'unread', since: now - 10 * 60e3, quiet: false, title: 'Quiet B' },
    { status: 'idle', since: now - 5 * 60e3, quiet: true, title: 'Quiet C' },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.quiet = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = false;
          s.stalledSince = null;
          s.quiet = p.quiet || undefined;
          s.title = p.title;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var status = function () { var t = document.querySelector('.ca-sessinsp-head'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chip = function (i) { var r = row(i); return !!r && !!r.querySelector('.ca-quietchip'); };
  var asked = function () { return calls('quiet_sessions').map(function (c) { return c[1]; }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return on(plan[i].title).then(function () { return wait(50); });
  };

  say('quiet check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000, 'three sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () { return show(0); })
    .then(function () { return menuOf(row(0)); })
    .then(function (labels) {
      var at = labels.indexOf('Quiet');
      check('a turn that ended has Quiet in its menu, unchecked', at >= 0, labels.join(' | '));
      check('right after Snooze', at > 0 && /^Snooze › /.test(labels[at - 1]), labels.slice(Math.max(0, at - 2), at + 1).join(' | '));
      check('its row has no Quiet chip', !chip(0));
      menuItem('Quiet')();
      return on('Quiet B');
    })
    .then(function () {
      var a = asked()[0];
      check('Quiet asks the engine to quiet it', !!a && a.keys.length === 1 && a.keys[0] === picked[0].key && a.quiet === true, JSON.stringify(a));
      check('and goes on to the next that waits on you', title() === 'Quiet B', title());
      check('the strip says how that went', /Quiet A: quieted\./.test(strip()), strip());
      check('its row shows it idle, with the Quiet chip, at once', chip(0) && row(0).classList.contains('is-idle'), row(0).className);
      var c = row(0).querySelector('.ca-quietchip');
      check('whose tooltip says what still comes to you', !!c && /needs you/.test(c.title), c ? c.title : '');
      return show(0);
    })
    .then(function () {
      check('its header says it’s quiet', /Idle · .+ · quiet/.test(status()), status());
      return menuOf(row(0));
    })
    .then(function (labels) {
      check('its menu has Quiet checked now', labels.indexOf('Quiet ✓') >= 0, labels.join(' | '));
      check('a quiet one from the board shows its chip', chip(2));
      return menuOf(row(2));
    })
    .then(function (labels) {
      check('and Quiet checked in its menu', labels.indexOf('Quiet ✓') >= 0, labels.join(' | '));
      menuItem('Quiet ✓')();
      return until(function () { return asked().length === 2; }, 3000, 'unchecking it asking the engine').then(function () { return wait(100); });
    })
    .then(function () {
      var a = asked()[1];
      check('unchecking it asks the engine to make it heard again', !!a && a.keys[0] === picked[2].key && a.quiet === false, JSON.stringify(a));
      check('and its chip goes at once', !chip(2));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
