// Checks that Sessions' panel follows a session you move to another account to its copy (App.tsx
// `moving`, menus.ts `moving`), rather than going on to the next that waits: a Claude Code
// session its usage limit stopped, with another session waiting after it, is moved by its
// header's button. The engine archives it before the move answers, and lists the copy a moment
// after: meanwhile the panel holds it as it was, and then the copy is selected and shown.
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
  var picked = null;
  var other = null;
  var next = null;
  var copyKey = null;
  // What the engine has done so far: nothing, the session archived, then its copy listed.
  var stage = 'before';
  var shape = function (board) {
    board.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        s.snoozedUntil = undefined;
        if (l.hidden || l.device || s.visitor || s.archived) return;
        if (!picked && s.harness === 'claude' && s.resumeCommand) {
          var others = accounts.filter(function (a) { return a.name !== s.account; });
          if (others.length !== 1) return;
          other = others[0].name;
          picked = { key: s.key, lane: l.id, account: s.account };
          copyKey = 'claude:' + other + ':f0000000-0000-4000-8000-000000000000';
        }
        if (picked && s.key === picked.key) {
          s.title = 'Move me';
          s.status = 'error';
          s.statusSince = now - 10 * 60e3;
          s.lastActivity = now - 10 * 60e3;
          s.live = false;
          s.hosted = false;
          s.surface = 'cli';
          s.excerpt = 'You’ve hit your session limit · resets 11:40am (America/Los_Angeles)';
          s.limit = { meter: 'session', resetsAt: now + 3600e3, spendCapped: false, at: now - 10 * 60e3, moveTo: other };
          s.archived = stage !== 'before';
          if (stage === 'copied') {
            var copy = JSON.parse(JSON.stringify(s));
            Object.assign(copy, { key: copyKey, id: copyKey.split(':')[2], account: other, title: 'Moved copy', status: 'working', statusSince: now, lastActivity: now, archived: false, limit: null, live: true, origin: { key: s.key, how: 'fork' } });
            l.sessions.push(copy);
          }
        } else if (picked && !next) {
          // The next that waits on you, after it.
          next = { key: s.key, lane: l.id };
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
  var fresh = function () { return real('get_board').then(shape); };
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
  var row = function (lane, key) { return document.getElementById('row-' + lane + '::' + key); };
  var selected = function () { var r = document.querySelector('[role="listbox"] .is-selected[id^="row-"]'); return r ? r.id : null; };
  var push = function (to) { stage = to; return fresh().then(function (b) { window.__emit('board', b); return wait(300); }); };

  say('move follow check: waiting for the list');
  until(function () { return !!picked && !!next && row(picked.lane, picked.key); }, 20000, 'a Claude Code session with exactly one other Claude Code account to move to, and another session')
    .then(function () { return wait(300); })
    .then(function () {
      row(picked.lane, picked.key).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Move me'; }, 3000, 'its panel');
    })
    .then(function () {
      var button = document.querySelector('.ca-sessinsp-head .ca-sessinsp-limit button');
      check('its header offers the move', !!button && button.textContent === 'Move to ' + other + ' and go on', button ? button.textContent : 'no button');
      button.click();
      return until(function () { return !!resolveMove; }, 3000, 'the move asked for');
    })
    .then(function () { return push('archived'); })
    .then(function () {
      check('archived while it moves, it stays in the panel', title() === 'Move me', title());
      check('and nothing else is selected', selected() === null || selected() === 'row-' + picked.lane + '::' + picked.key, selected());
      resolveMove({ key: copyKey, message: 'Moved “Move me” to ' + other + ': it carries on in a new terminal.' });
      return wait(300);
    })
    .then(function () {
      check('the move done, it waits for the copy rather than going on to the next', title() === 'Move me', title() + ' / ' + strip());
      check('and the strip says nothing of it leaving', !/archived\./.test(strip()), strip());
      return push('copied');
    })
    .then(function () { return until(function () { return title() === 'Moved copy'; }, 3000).catch(function () {}); })
    .then(function () {
      check('once the copy is listed, the panel shows it', title() === 'Moved copy', title());
      check('selected in the list', selected() === 'row-' + picked.lane + '::' + copyKey, selected());
      check('not the next that waits', title() !== 'Next in line', title());
    })
    .then(done, stopped);
})();
