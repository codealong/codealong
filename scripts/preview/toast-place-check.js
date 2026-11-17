// Checks where the main window says things around a session's panel (Toasts.tsx, UsageBar.tsx
// `toast`, MainPanel.tsx): a message that didn't go after ⌘⏎ went on to the next is said on the
// next one's strip, not in a toast over its box, and in a toast where no strip shows by then
// (several picked); a toast is a line in the usage bar, over neither the list nor the panel's
// message box, with the panel taking the whole window too; the account chips step down to give it
// the room its words need (up to its limit) and come back once it goes; and it stays while the
// pointer is on it, going once the pointer leaves.
//   PREVIEW_SIZE=1200,900 PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/toast-place-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var picked = [];
  // Every message fails, when the check says: once the panel has gone on from it.
  var flying = [];
  var fail = function () { if (flying.length) flying.shift()('Its terminal is gone.'); };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // A message to one that isn't running resumes it with the message (ahead.ts `sendAhead`).
    if (cmd === 'send_to_session' || cmd === 'resume_session') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (_, reject) { flying.push(reject); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= 2) return;
          seen[s.key] = true;
          s.status = 'unread';
          s.statusSince = now - (2 - picked.length) * 3600e3;
          s.lastActivity = s.statusSince;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Toast ' + 'AB'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var toast = function () { return document.querySelector('.ca-toast'); };
  var rect = function (el) { var r = el.getBoundingClientRect(); return { left: Math.round(r.left), right: Math.round(r.right), top: Math.round(r.top), bottom: Math.round(r.bottom) }; };
  var overlaps = function (a, b) { return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom; };
  var inside = function (a, b) { return a.left >= b.left && a.right <= b.right && a.top >= b.top && a.bottom <= b.bottom; };
  var bar = function () { return rect(document.querySelector('.ca-usagebar')); };
  var tight = function () { return document.querySelectorAll('.ca-usagebar-accts .ca-uchip.is-tight').length; };
  var chips = function () { return document.querySelectorAll('.ca-usagebar-accts .ca-uchip').length; };
  // What the toast's words would take on one line, with its icon and Undo: its width, less the
  // words' room, plus all of them.
  var needs = function () { var t = toast(); var w = t.querySelector('.ca-oneline-text'); return t.offsetWidth - w.clientWidth + w.scrollWidth; };
  var hover = function (on) { toast().dispatchEvent(new MouseEvent(on ? 'mouseover' : 'mouseout', { bubbles: true, relatedTarget: on ? document.body : document.querySelector('.ca-board') })); };
  var tightBefore = 0;
  var sentFrom = 0;
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var archived = function () {
    window.__emit('agent-done', { key: picked[1].key, title: 'Toast B', message: '“Orchestrator” marked “Toast B” done: its terminal is closed and it’s archived.' });
    return until(toast, 3000, 'the toast').then(function () { return wait(100); });
  };

  say('toast place check: waiting for the list');
  until(function () { return picked.length === 2 && row(0); }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Toast A').then(function () { return until(box, 5000, 'the box'); });
    })
    .then(function () {
      box().focus();
      type(box(), 'Run the tests again');
      return wait(50);
    })
    .then(function () {
      box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
      return on('Toast B');
    })
    .then(function () {
      check('⌘⏎ goes on to the next', title() === 'Toast B', title());
      fail();
      return until(function () { return /Didn’t send/.test(strip()); }, 3000).catch(function () {});
    })
    .then(function () {
      check('a message that didn’t go is said on the strip of the one gone on to', /Didn’t send to Toast A, so it’s back in its box: Its terminal is gone\./.test(strip()), strip());
      check('not in a toast', !toast(), toast() && toast().textContent);
      tightBefore = tight();
      note(chips() + ' account chips, ' + tightBefore + ' showing only a number before the toast');
      return archived();
    })
    .then(function () {
      var t = rect(toast());
      var board = rect(document.querySelector('.ca-board'));
      var composer = rect(document.querySelector('.ca-sessinsp .ca-composer'));
      var accts = rect(document.querySelector('.ca-usagebar-accts'));
      var end = rect(document.querySelector('.ca-usagebar-end'));
      check('a toast is a line in the usage bar', inside(t, bar()), JSON.stringify({ toast: t, bar: bar() }));
      check('between the chips and the bar’s end', accts.right <= t.left && t.right <= end.left, JSON.stringify({ chips: accts, toast: t, end: end }));
      check('over neither the list nor the panel’s message box', !overlaps(t, board) && !overlaps(t, composer), JSON.stringify({ toast: t, board: board, box: composer }));
      var want = Math.min(needs(), 560);
      note('its words want ' + needs() + 'px; it has ' + (t.right - t.left) + 'px, and ' + tight() + ' of ' + chips() + ' chips show only a number');
      check('the chips step down to give it the room its words need, up to its limit', t.right - t.left >= want - 1 || tight() === chips(), JSON.stringify({ toast: t.right - t.left, wants: want, tight: tight(), chips: chips() }));
      hover(true);
      return wait(9000);
    })
    .then(function () {
      check('the pointer on it holds it past its time', !!toast(), 'gone');
      hover(false);
      return until(function () { return !toast(); }, 10000).catch(function () {});
    })
    .then(function () {
      check('and once the pointer leaves, it goes', !toast(), toast() && toast().textContent);
      return wait(300);
    })
    .then(function () {
      check('the chips come back as they were', tight() === tightBefore, tight() + ' showing only a number, ' + tightBefore + ' before');
      return archived();
    })
    .then(function () {
      document.querySelector('.ca-sessinsp-max').click();
      return until(function () { return q('.ca-win-body.is-panel-max'); }, 3000, 'the panel taking the window').then(function () { return wait(200); });
    })
    .then(function () {
      var t = rect(toast());
      var composer = rect(document.querySelector('.ca-sessinsp .ca-composer'));
      check('with the panel taking the window, the toast is still in the bar, clear of its box', inside(t, bar()) && !overlaps(t, composer), JSON.stringify({ toast: t, bar: bar(), box: composer }));
      document.querySelector('.ca-sessinsp-max').click();
      return until(function () { return !q('.ca-win-body.is-panel-max'); }, 3000, 'the list back').then(function () { return wait(200); });
    })
    .then(function () {
      return until(box, 3000, 'Toast B’s box');
    })
    .then(function () {
      box().focus();
      type(box(), 'And the lint');
      return wait(50);
    })
    .then(function () {
      sentFrom = window.__calls.length;
      box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
      return on('Toast A');
    })
    .then(function () {
      row(1).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, metaKey: true }));
      return until(function () { return q('.ca-pick'); }, 3000, 'the two picked');
    })
    .then(function () {
      check('⌘⏎ sent from Toast B and went on', flying.length === 1, flying.length + ' messages in flight; sent since: ' + window.__calls.slice(sentFrom).map(function (c) { return c[0]; }).join(' '));
      fail();
      return until(function () { return toast() && /Didn’t send to Toast B/.test(toast().textContent); }, 3000).catch(function () {});
    })
    .then(function () {
      var t = toast();
      check('with several picked by then, a message that didn’t go is said in a toast', !!t && /Didn’t send to Toast B, so it’s back in its box: Its terminal is gone\./.test(t.textContent), t ? t.textContent : 'no toast');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
