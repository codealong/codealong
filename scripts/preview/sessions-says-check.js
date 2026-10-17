// Checks that what a session's box has to say is in the box's own foot in the Sessions view's
// panel (SessionReply.tsx `BoxSays`), between who it goes to and the buttons, on one line, and not
// on the strip over its header (SessionPanel.tsx `QueueStrip`) nor under the box: with nothing else
// waiting on you and its turn running where it can take a message at once, it says ⌃⏎ sends now;
// a command led by `!` says what ⏎ does with it, cut short where the foot is too narrow for it
// and whole under the pointer (primitives.tsx `OneLine`); a message on its way says Sending…, then
// how it went; one that didn't go says why, in the alert color, and is back in the box. The box's
// foot has its mode, its account and how full its context is at the left, level with the model
// and Send at the right, and nothing is said under the box.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/sessions-says-check.js
//   PREVIEW_SCHEME=dark PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/sessions-says-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  // The sends, each answered when the check says (`answer`), so what shows while one is on its way
  // can be seen.
  var pending = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  var running = function (s) {
    s.status = 'working';
    s.statusSince = Date.now() - 60e3;
    s.live = true;
    s.hosted = false;
    s.stoppable = true;
    s.steerable = true;
    s.asks = false;
    s.suggestions = undefined;
    s.mode = { kind: 'skips', label: 'Bypass', detail: 'Runs every tool without asking.' };
    s.context = { used: 300000, window: 1000000 };
    s.title = 'Says A';
    return s;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (resolve, reject) { pending.push({ resolve: resolve, reject: reject }); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // Nothing waits on you, so the strip has nothing of the queue's to say.
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked && s.key === picked.key) running(s);
          if (picked || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          running(s);
          picked = { key: s.key, lane: l.id };
          window.__madeSessions = {};
          window.__madeSessions[s.key] = s;
        });
      });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var noteEl = function () { return q('.ca-sessinsp .ca-composer-footrow > .ca-composer-note'); };
  var stripNote = function () { var n = q('.ca-sessinsp .ca-reply-queue-note'); return n ? n.textContent : ''; };
  var note = function () { var n = noteEl(); return n ? n.textContent : ''; };
  var under = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) > .ca-composer-note'); };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var says = function (what, why) { return until(function () { return what.test(note()); }, 3000, why); };

  say('sessions says check: waiting for the list');
  until(function () { return row(); }, 20000, 'no Claude Code session to make run')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Says A' && !!box(); }, 5000, 'its panel');
    })
    .then(function () { return says(/send now/, 'the box’s foot saying the key'); })
    .then(function () {
      check('the box’s foot says ⌃⏎ sends now, into the turn it’s taking', note() === '⌃⏎ send now, into the turn it’s taking', note());
      check('and the strip says none of it', stripNote() === '', stripNote());
      check('nothing is said under the box', !under(), under() && under().textContent);
      check('the strip counts nothing of what’s left', !q('.ca-sessinsp .ca-reply-queue .ca-chip'));

      // The box's foot: who a message goes to at the left, the model and Send at the right.
      var field = q('.ca-sessinsp .ca-composer-field');
      var who = q('.ca-composer-who', field);
      var buttons = q('.ca-composer-buttons', field);
      var names = who ? all('.ca-mode, .ca-account, .ca-context', who).map(function (e) { return e.className.split(' ')[0]; }) : [];
      check('the box’s foot has the mode, the account and the context, in that order', names.join(' ') === 'ca-mode ca-account ca-context', names.join(' '));
      var w = who.getBoundingClientRect();
      var b = buttons.getBoundingClientRect();
      var f = field.getBoundingClientRect();
      var t = q('textarea', field).getBoundingClientRect();
      check('at the left, under where you write', w.left - f.left < 16 && w.top >= t.bottom - 1, Math.round(w.left - f.left) + ' in, ' + Math.round(w.top - t.bottom) + ' under');
      check('level with the buttons at the right', Math.abs((w.top + w.bottom) / 2 - (b.top + b.bottom) / 2) <= 2 && w.right <= b.left, Math.round(w.top) + '–' + Math.round(w.bottom) + ' beside ' + Math.round(b.top) + '–' + Math.round(b.bottom));
      check('inside the box', w.bottom <= f.bottom && b.bottom <= f.bottom, Math.round(Math.max(w.bottom, b.bottom)) + ' in ' + Math.round(f.bottom));
      var n = noteEl().getBoundingClientRect();
      check('what it says is between them, level with them', n.left >= w.right - 1 && n.right <= b.left + 1 && Math.abs((n.top + n.bottom) / 2 - (w.top + w.bottom) / 2) <= 2, Math.round(n.left) + '–' + Math.round(n.right) + ' between ' + Math.round(w.right) + ' and ' + Math.round(b.left));

      type(box(), '!ls');
      return says(/runs it in a new/, 'what ⏎ does with a command');
    })
    .then(function () {
      check('a command led by ! says what ⏎ does with it', /^⏎ runs it in a new .+ tab, as you; .+ is told what it prints once it ends\.$/.test(note()), note());
      // Narrowed so it can't fit, it's cut on its one line, and has all of it to show under the
      // pointer.
      var el = noteEl();
      var line = el.getBoundingClientRect().height;
      el.style.maxWidth = '120px';
      return wait(100).then(function () {
        check('cut short where it doesn’t fit, still one line', el.getBoundingClientRect().height <= line + 1 && el.getAttribute('data-whole') === note(), Math.round(el.getBoundingClientRect().height) + ' tall, whole: ' + el.getAttribute('data-whole'));
        var shown = getComputedStyle(el, '::after');
        check('with all of it over the box’s foot to show under the pointer', shown.position === 'absolute' && shown.bottom !== 'auto' && shown.whiteSpace === 'normal', shown.position + ' ' + shown.bottom + ' ' + shown.whiteSpace);
        // As wide as the box, not as the little room the line has, so it isn't stood up a word to
        // a line.
        var field = q('.ca-sessinsp .ca-composer-field').getBoundingClientRect();
        check('across the box’s whole width', Math.abs(parseFloat(shown.width) - (field.width - 8)) <= 3, shown.width + ' in a box ' + Math.round(field.width) + ' wide');
        el.style.maxWidth = '';
      });
    })
    .then(function () {
      type(box(), 'Also check the docs');
      key(box(), { key: 'Enter' });
      return says(/^Sending…$/, 'Sending…');
    })
    .then(function () {
      check('a message on its way says so', note() === 'Sending…' && box().value === '', note() + ' / ' + box().value);
      check('whole, with nothing more to show under the pointer', noteEl().getAttribute('data-whole') === null, String(noteEl().getAttribute('data-whole')));
      check('and nothing under the box', !under());
      pending[0].resolve({ message: 'Pasted into its background job.', opened: false });
      return says(/^Pasted into its background job\.$/, 'how it went');
    })
    .then(function () {
      check('then how it went', note() === 'Pasted into its background job.' && !noteEl().classList.contains('is-error'), note());
      var quiet = getComputedStyle(noteEl()).color;
      type(box(), 'This one fails');
      key(box(), { key: 'Enter' });
      return until(function () { return pending.length === 2; }, 3000, 'the second send').then(function () {
        pending[1].reject('Its terminal is gone.');
        return says(/Its terminal is gone/, 'why it didn’t go');
      }).then(function () {
        check('one that didn’t go says why', note() === 'Its terminal is gone.', note());
        check('in the alert color', noteEl().classList.contains('is-error') && getComputedStyle(noteEl()).color !== quiet, getComputedStyle(noteEl()).color + ' / ' + quiet);
        check('and is back in the box', box().value === 'This one fails', box().value);
        check('still nothing under the box', !under());
      });
    })
    .then(done)
    .catch(stopped);
})();
