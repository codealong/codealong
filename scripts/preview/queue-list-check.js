// Checks the lists behind the strip's counts in Sessions' panel (SessionPanel.tsx `QueueList`):
// three sessions that need you after the one shown and one whose turn ended. Pointing at a count
// lists what it counts under it, the next first, each with where it is, and the list goes once
// the pointer leaves; a click keeps it up until a press elsewhere. A row picked shows its session
// as ⌘] would, saying nothing on the strip, and ⌘] goes on from there. A right-click on a row
// doesn't give the shown session's menu.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/queue-list-check.js
(function () {
  // This runs before the app's script: the board it gets has five sessions waiting on you, made
  // from real ones, and nothing else waiting.
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 2 * 3600e3 },
    { status: 'error', since: now - 3600e3 },
    { status: 'needs', since: now - 5 * 60e3 },
    { status: 'unread', since: now - 10 * 60e3 },
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
          if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Queue ' + 'ABCDE'[picked.length];
          picked.push({ key: s.key, lane: l.id, callsign: l.callsign, account: s.account });
        });
      });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var strip = function () { return q('.ca-sessinsp .ca-reply-queue'); };
  var count = function (w) { return q('.ca-sessinsp .ca-reply-queue-count[data-waits="' + w + '"]'); };
  var list = function () { return q('.ca-sessinsp .ca-queue-list'); };
  var rows = function () { return all('.ca-queue-list-row', list() || document.createElement('div')); };
  var titles = function () { return rows().map(function (r) { return q('.ca-queue-list-title', r).textContent; }).join(', '); };
  var enter = function (el) { el.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
  };

  say('queue list check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return wait(50); });
    })
    .then(function () {
      var needs = count('needs');
      check('each count is a button that says what it counts', !!needs && needs.tagName === 'BUTTON' && /^3 need you after this one$/.test(needs.getAttribute('aria-label')), needs && needs.getAttribute('aria-label'));
      check('the counts have no tooltip to cover their list', !q('.ca-sessinsp .ca-reply-queue-counts').title && !needs.title);
      enter(needs);
      return wait(100);
    })
    .then(function () {
      check('a pass over a count shows nothing yet', !list());
      return wait(300);
    })
    .then(function () {
      check('resting on a count lists what it counts, the next first', titles() === 'Queue B, Queue C, Queue D', titles());
      check('it says how many, as the count does', q('.ca-queue-list-head', list()).textContent === '3 need you after this one', q('.ca-queue-list-head', list()).textContent);
      var where = q('.ca-queue-list-where', rows()[0]);
      var b = picked[1];
      check('each row says where it is, its account last', !!where && where.textContent === [b.callsign, b.account].filter(Boolean).join(' · '), where && where.textContent);
      check('each row has its status’s glyph', !!q('.ca-glyph--error', rows()[1]) && !!q('.ca-glyph--needs', rows()[0]));
      var c = count('needs').getBoundingClientRect();
      var l = list().getBoundingClientRect();
      var panel = q('.ca-sessinsp').getBoundingClientRect();
      check('under its count, within the panel', l.top >= c.bottom && l.top - c.bottom < 10 && l.left >= panel.left && l.right <= panel.right && l.bottom <= panel.bottom, JSON.stringify([c.bottom, l.top, l.left, l.right, panel.left, panel.right]));
      check('the count shows its list is up', count('needs').classList.contains('is-open') && count('needs').getAttribute('aria-expanded') === 'true');
      check('a press on it doesn’t drag the waiting window, whose title bar the strip is', list().getAttribute('data-tauri-drag-region') === 'false');
      window.note('list at ' + Math.round(l.left) + ',' + Math.round(l.top) + ' ' + Math.round(l.width) + '×' + Math.round(l.height));
      // Over to the list: it stays.
      leave(count('needs'));
      enter(list());
      return wait(400);
    })
    .then(function () {
      check('it stays while the pointer is on it', !!list());
      enter(count('unread'));
      return wait(20);
    })
    .then(function () {
      check('pointing at another count shows its list at once', titles() === 'Queue E', titles());
      leave(count('unread'));
      return wait(400);
    })
    .then(function () {
      check('it goes once the pointer leaves', !list());
      count('needs').click();
      return wait(20);
    })
    .then(function () {
      check('a click shows the list at once', titles() === 'Queue B, Queue C, Queue D', titles());
      leave(count('needs'));
      return wait(400);
    })
    .then(function () {
      check('clicked, it stays when the pointer leaves', !!list());
      var menusBefore = menus();
      rightClick(rows()[1]);
      check('a right-click on a row gives no menu, not the shown session’s', menus() === menusBefore && !!list());
      press(document.body);
      return wait(50);
    })
    .then(function () {
      check('a press elsewhere puts it away', !list());
      count('needs').click();
      return wait(20);
    })
    .then(function () {
      rows()[1].click();
      return on('Queue C').then(function () { return wait(50); });
    })
    .then(function () {
      check('a row picked shows its session', title() === 'Queue C', title());
      check('and the list goes', !list());
      check('the strip says nothing of it, as after ⌘]', !q('.ca-sessinsp .ca-reply-queue-note.is-notice'));
      var box = q('.ca-sessinsp .ca-composer textarea');
      check('its box has the cursor, as a row clicked in the list', document.activeElement === box);
      key(box, { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      check('⌘] goes on from the one picked', title() === 'Queue D', title());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
