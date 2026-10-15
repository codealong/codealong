// Checks that the strip over a session's panel in Sessions says whichever came last of what the
// window said and what the box said (SessionPanel.tsx `StripNote`): ⌘⏎ sends to the first and goes
// on to the second, whose box is given a command led by `!`, so the strip says what ⏎ does with
// it; then the first's message fails, and the strip says so over what the box said before; then
// the box says something new (what a `/` command typed whole does), and that's what the strip says.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/strip-last-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = [];
  var pending = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (resolve, reject) { pending.push({ resolve: resolve, reject: reject }); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var now = Date.now();
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // Only the two made here wait on you.
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          var at = picked.map(function (x) { return x.key; }).indexOf(s.key);
          if (at < 0 && picked.length < 2) {
            picked.push({ key: s.key, lane: l.id });
            at = picked.length - 1;
          }
          if (at < 0) return;
          s.status = 'unread';
          s.statusSince = now - (2 - at) * 600e3;
          s.lastActivity = s.statusSince;
          s.live = true;
          s.hosted = false;
          s.asks = false;
          s.suggestions = undefined;
          s.title = at === 0 ? 'Strip A' : 'Strip B';
        });
      });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var noteEl = function () { return q('.ca-sessinsp .ca-reply-queue-note'); };
  var note = function () { var n = noteEl(); return n ? n.textContent : ''; };
  var row = function () { return picked[0] && document.getElementById('row-' + picked[0].lane + '::' + picked[0].key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var says = function (what, why) { return until(function () { return what.test(note()); }, 4000, why); };

  say('strip last check: waiting for the list');
  until(function () { return picked.length === 2 && row(); }, 20000, 'two Claude Code sessions to make wait')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Strip A' && !!box(); }, 5000, 'the first');
    })
    .then(function () {
      type(box(), 'Go ahead.');
      key(box(), { key: 'Enter', metaKey: true });
      return until(function () { return title() === 'Strip B' && !!box(); }, 5000, 'on to the second');
    })
    .then(function () {
      type(box(), '!ls');
      return says(/runs it in a new/, 'what ⏎ does with a command');
    })
    .then(function () {
      check('the box’s command says what ⏎ does, over how the last one went', /^⏎ runs it in a new /.test(note()), note());
      pending[0].reject('Its terminal is gone.');
      return says(/Didn’t send to Strip A/, 'the failure');
    })
    .then(function () {
      check('a message that then fails says so over what the box said before', /^Didn’t send to Strip A, so it’s back in its box: Its terminal is gone\./.test(note()) && noteEl().classList.contains('is-error'), note());
      type(box(), '/fork now');
      return says(/^\/fork/, 'the box’s newer word');
    })
    .then(function () {
      check('and what the box says after that is said over it', /^\/fork /.test(note()) && !noteEl().classList.contains('is-error'), note());
      check('nothing is said under the box', !q('.ca-sessinsp .ca-composer:not(.is-approval) > .ca-composer-note'));
    })
    .then(done)
    .catch(stopped);
})();
