// Checks that Sessions' panel follows a session you move to another account to its copy (App.tsx
// `moving`, menus.ts `moving`), rather than going on to the next that waits: a Claude Code
// session its usage limit stopped, with another session waiting after it, is moved by its
// header's button. The engine archives it before the move answers, and lists the copy a moment
// after: meanwhile the panel holds it as it was, offering no second move, and then the copy is
// selected and shown. Then another, whose worktree has a PR, is moved with Sessions filtered to
// Needs you, which leaves its working copy out, and has its PR opened from its chip while it moves:
// the PR stays in the panel as the copy is shown, and its way back goes to the copy.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/move-follow-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };
  var accounts = get('./accounts.json').filter(function (a) { return a.harness === 'claude' && a.signedIn; });
  var now = Date.now();
  // The sessions moved, each with its copy: the first any, the second one whose worktree has a
  // PR the board lists. Picked from the first board, and shaped the same on every one after.
  var movers = null;
  var next = null;
  // What the engine has done so far: the sessions it archived, and the copies it lists.
  var archived = {};
  var copied = {};
  var pick = function (board) {
    var prs = {};
    board.prs.forEach(function (p) { prs[p.url] = true; });
    var eligible = [];
    board.lanes.forEach(function (l) {
      if (l.hidden || l.device) return;
      l.sessions.forEach(function (s) {
        if (s.visitor || s.archived || s.harness !== 'claude' || !s.resumeCommand) return;
        var others = accounts.filter(function (a) { return a.name !== s.account; });
        if (others.length === 1) eligible.push({ key: s.key, lane: l.id, other: others[0].name, pr: !!(l.pr && prs[l.pr.url]) });
      });
    });
    var withPr = eligible.filter(function (m) { return m.pr; })[0];
    var first = eligible.filter(function (m) { return m !== withPr; })[0];
    movers = [first, withPr].filter(Boolean).map(function (m, i) {
      return Object.assign(m, { title: i ? 'Move me too' : 'Move me', copyTitle: i ? 'Moved copy too' : 'Moved copy', copy: 'claude:' + m.other + ':f000000' + i + '-0000-4000-8000-000000000000' });
    });
  };
  var shape = function (board) {
    if (!movers) pick(board);
    board.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        s.snoozedUntil = undefined;
        if (l.hidden || l.device || s.visitor || s.archived) return;
        var m = movers.filter(function (x) { return x.key === s.key; })[0];
        if (m) {
          s.title = m.title;
          s.status = 'error';
          s.statusSince = now - 10 * 60e3;
          s.lastActivity = now - 10 * 60e3;
          s.live = false;
          s.hosted = false;
          s.surface = 'cli';
          s.excerpt = 'You’ve hit your session limit · resets 11:40am (America/Los_Angeles)';
          s.limit = { meter: 'session', resetsAt: now + 3600e3, spendCapped: false, at: now - 10 * 60e3, moveTo: m.other };
          s.archived = !!archived[m.key];
          if (copied[m.key]) {
            var copy = JSON.parse(JSON.stringify(s));
            Object.assign(copy, { key: m.copy, id: m.copy.split(':')[2], account: m.other, title: m.copyTitle, status: 'working', statusSince: now, lastActivity: now, archived: false, limit: null, live: true, origin: { key: s.key, how: 'fork' } });
            l.sessions.push(copy);
          }
        } else if (!next && movers.length) {
          // The next that waits on you.
          next = s.key;
          s.title = 'Next in line';
          s.status = 'needs';
          s.asks = true;
          s.statusSince = now - 60e3;
          s.lastActivity = now - 60e3;
        }
      });
    });
    return board;
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  var resolveMove = null;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'move_session') {
      window.__calls.push([cmd, args]);
      return new Promise(function (r) { resolveMove = r; });
    }
    var p = real.apply(this, arguments);
    return cmd === 'get_board' ? p.then(shape) : p;
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var rowOf = function (m, key) { return document.getElementById('row-' + m.lane + '::' + (key || m.key)); };
  var selected = function () { var r = document.querySelector('[role="listbox"] .is-selected[id^="row-"]'); return r ? r.id : null; };
  var moveButton = function () { return document.querySelector('.ca-sessinsp-head .ca-sessinsp-limit button'); };
  var push = function () { return real('get_board').then(shape).then(function (b) { window.__emit('board', b); return wait(300); }); };
  var click = function (el) { ['mousedown', 'mouseup', 'click'].forEach(function (t) { el.dispatchEvent(new MouseEvent(t, { bubbles: true, cancelable: true, detail: 1 })); }); };
  // Selects a mover and moves it by its header's button, up to the engine archiving it.
  var startMove = function (m) {
    resolveMove = null;
    rowOf(m).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === m.title; }, 3000, m.title + '’s panel').then(function () {
      var button = moveButton();
      check(m.title + ': its header offers the move', !!button && button.textContent === 'Move to ' + m.other + ' and go on', button ? button.textContent : 'no button');
      // Twice, as a double-click would.
      button.click();
      button.click();
      return until(function () { return !!resolveMove; }, 3000, 'the move asked for');
    }).then(function () {
      archived[m.key] = true;
      return push();
    });
  };

  say('move follow check: waiting for the list');
  until(function () { return movers && movers.length && next && rowOf(movers[0]); }, 20000, 'a Claude Code session with exactly one other Claude Code account to move to, and another session')
    .then(function () { return wait(300); })
    .then(function () {
      var m = movers[0];
      return startMove(m).then(function () {
        check('a second click on the button moves nothing again', calls('move_session').length === 1, JSON.stringify(sent('move_session')));
        check('archived while it moves, it stays in the panel', title() === m.title, title());
        check('and nothing else is selected', selected() === null || selected() === 'row-' + m.lane + '::' + m.key, selected());
        resolveMove({ key: m.copy, message: 'Moved “' + m.title + '” to ' + m.other + ': it carries on in a new terminal.' });
        return wait(300);
      }).then(function () {
        check('the move done, it waits for the copy rather than going on to the next', title() === m.title, title() + ' / ' + strip());
        check('and the strip says nothing of it leaving', !/archived\./.test(strip()), strip());
        check('held, it offers no second move', !moveButton(), moveButton() ? moveButton().textContent : '');
        copied[m.key] = true;
        return push();
      }).then(function () { return until(function () { return title() === m.copyTitle; }, 3000).catch(function () {}); })
        .then(function () {
          check('once the copy is listed, the panel shows it', title() === m.copyTitle, title());
          check('selected in the list', selected() === 'row-' + m.lane + '::' + m.copy, selected());
        });
    })
    .then(function () {
      var m = movers[1];
      if (!m) return note('no session whose worktree has a PR the board lists, to open its PR while it moves');
      var prPanel = function () { return document.querySelector('.ca-prpanel'); };
      var needsYou = function () { return Array.prototype.filter.call(document.querySelectorAll('.ca-side-chip'), function (c) { return /^Needs you/.test(c.textContent); })[0]; };
      var filter = needsYou();
      if (!filter) throw new Error('no Needs you filter in the sidebar');
      filter.click();
      return until(function () { return needsYou().getAttribute('aria-pressed') === 'true' && rowOf(m); }, 3000, m.title + ' listed under Needs you').then(function () {
        return startMove(m);
      }).then(function () {
        var chip = document.querySelector('.ca-sessinsp-head .ca-pr');
        if (!chip) throw new Error('no PR chip in ' + m.title + '’s header');
        click(chip);
        return until(prPanel, 3000, 'its PR in the panel');
      }).then(function () {
        resolveMove({ key: m.copy, message: 'Moved “' + m.title + '” to ' + m.other + ': it carries on in a new terminal.' });
        copied[m.key] = true;
        return wait(300).then(push);
      }).then(function () {
        check('a PR opened from its chip while it moved stays in the panel as the copy comes', !!prPanel());
        check('the copy, which the filter left out, is shown and selected under it', selected() === 'row-' + m.lane + '::' + m.copy, selected());
        var back = document.querySelector('.ca-prpanel .ca-panelback');
        check('and the way back goes to the copy', !!back && back.textContent.indexOf(m.copyTitle) >= 0, back ? back.textContent : 'no way back');
        if (back) back.click();
        return until(function () { return title() === m.copyTitle; }, 3000).catch(function () {});
      }).then(function () {
        check('back, the copy’s panel shows', title() === m.copyTitle, title());
      });
    })
    .then(done, stopped);
})();
