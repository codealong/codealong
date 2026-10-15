// Checks that the waiting window's strip says whichever came last of what the window said and what
// the box said (SessionPanel.tsx `StripNote`), the same words said again included: an open that
// fails says why on the strip; a command led by `!` typed in the box then says what ⏎ does with
// it there; the same open failing again with the same words is said over that, not left under
// it; and what the box says after that is said over it in turn.
//   PREVIEW_WAIT=40000 scripts/preview.sh waiting '' scripts/preview/waiting-strip-check.js
(function () {
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'open_target') {
      window.__calls.push([cmd, args, options]);
      return Promise.reject('Its terminal is gone.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // Only the one made here waits on you.
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked && s.key !== picked) return;
          if (!picked && (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude')) return;
          picked = s.key;
          s.status = 'unread';
          s.statusSince = Date.now() - 600e3;
          s.live = true;
          s.hosted = false;
          s.asks = false;
          s.suggestions = undefined;
          s.title = 'Strip W';
        });
      });
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-composer:not(.is-approval) textarea'); };
  var noteEl = function () { return q('.ca-reply-queue-note'); };
  var note = function () { var n = noteEl(); return n ? n.textContent : ''; };
  var open = function () { return q('.ca-sessinsp-tools .ca-split-main') || all('.ca-sessinsp-tools button').filter(function (b) { return /^Open in /.test(b.textContent); })[0]; };
  var opens = function () { return calls('open_target').length; };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var says = function (what, why) { return until(function () { return what.test(note()); }, 4000, why); };
  var failed = /^Its terminal is gone\.$/;

  say('waiting strip check: waiting for the window');
  until(function () { return title() === 'Strip W' && !!box() && !!open(); }, 20000, 'the session and its Open')
    .then(function () {
      open().click();
      return until(function () { return opens() === 1; }, 3000, 'the open').then(function () { return says(failed, 'why it didn’t open'); });
    })
    .then(function () {
      check('an open that fails says why on the strip', failed.test(note()) && noteEl().classList.contains('is-error'), note());
      type(box(), '!ls');
      return says(/runs it in a new/, 'what ⏎ does with a command');
    })
    .then(function () {
      check('a command typed after it says what ⏎ does, over it', /^⏎ runs it in a new /.test(note()), note());
      open().click();
      return until(function () { return opens() === 2; }, 3000, 'the second open').then(function () { return says(failed, 'the same failure again'); });
    })
    .then(function () {
      check('the same failure again is said over the box’s', failed.test(note()) && noteEl().classList.contains('is-error'), note());
      type(box(), '/fork now');
      return says(/^\/fork /, 'the box’s newer word');
    })
    .then(function () {
      check('and what the box says after that is said over it', /^\/fork /.test(note()) && !noteEl().classList.contains('is-error'), note());
    })
    .then(done)
    .catch(stopped);
})();
