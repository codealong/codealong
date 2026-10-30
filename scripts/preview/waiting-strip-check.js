// Checks that the waiting window's strip, its title bar, says what the window said and the box's
// foot what the box said (SessionPanel.tsx `StripNote`, SessionReply.tsx `BoxSays`), each in its
// own place: an open that fails says why on the strip, in the alert color; a command led by `!`
// typed in the box then says what ⏎ does with it in the box's foot, the strip still saying why
// the open failed; the same open failing again says so again. Cut short, what the strip says has
// all of it to show under the pointer, down from the strip and no wider than the strip from where
// its words start, clear of the window's buttons (primitives.tsx `OneLine`).
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
          s.raised = undefined;
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
  var stripEl = function () { return q('.ca-reply-queue-note'); };
  var strip = function () { var n = stripEl(); return n ? n.textContent : ''; };
  var note = function () { var n = q('.ca-composer-footrow > .ca-composer-note'); return n ? n.textContent : ''; };
  var open = function () { return q('.ca-sessinsp-tools .ca-split-main') || all('.ca-sessinsp-tools button').filter(function (b) { return /^Open in /.test(b.textContent); })[0]; };
  var opens = function () { return calls('open_target').length; };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var says = function (what, why) { return until(function () { return what.test(note()); }, 4000, why); };
  var stripSays = function (what, why) { return until(function () { return what.test(strip()); }, 4000, why); };
  var failed = /^Its terminal is gone\.$/;

  say('waiting strip check: waiting for the window');
  until(function () { return title() === 'Strip W' && !!box() && !!open(); }, 20000, 'the session and its Open')
    .then(function () {
      open().click();
      return until(function () { return opens() === 1; }, 3000, 'the open').then(function () { return stripSays(failed, 'why it didn’t open'); });
    })
    .then(function () {
      check('an open that fails says why on the strip', failed.test(strip()) && stripEl().classList.contains('is-error'), strip());
      type(box(), '!ls');
      return says(/runs it in a new/, 'what ⏎ does with a command');
    })
    .then(function () {
      check('a command typed after it says what ⏎ does in the box’s foot', /^⏎ runs it in a new /.test(note()), note());
      check('while the strip still says why the open failed', failed.test(strip()), strip());
      open().click();
      return until(function () { return opens() === 2; }, 3000, 'the second open').then(function () { return wait(200); });
    })
    .then(function () {
      check('the same failure again is still said on the strip', failed.test(strip()) && stripEl().classList.contains('is-error'), strip());
      // Narrowed so it can't fit, it's cut short, and what shows under the pointer goes down from
      // the strip, from where its words start to no further than the strip's end.
      var el = stripEl();
      el.style.maxWidth = '40px';
      return wait(100).then(function () {
        var bar = q('.ca-waiting .ca-reply > .ca-reply-queue').getBoundingClientRect();
        var r = el.getBoundingClientRect();
        var shown = getComputedStyle(el, '::after');
        var room = parseFloat(shown.maxWidth);
        check('cut short, it has all of it to show under the pointer', el.getAttribute('data-whole') === strip(), String(el.getAttribute('data-whole')));
        check('down from the strip', shown.position === 'absolute' && shown.top !== 'auto', shown.top);
        check('starting clear of the window’s buttons and ending at the strip’s end', r.left - 6 >= 69 && !isNaN(room) && r.left - 6 + room <= bar.right + 1 && r.left - 6 + room >= bar.right - 24, Math.round(r.left - 6) + ' + ' + Math.round(room) + ' against ' + Math.round(bar.right));
        el.style.maxWidth = '';
      });
    })
    .then(done)
    .catch(stopped);
})();
