// Checks the line at the end of a conversation while its agent works (Working.tsx, working.ts):
// the mark quiet (its C gray, its color gliding round it) and breathing, a word for what it's up to with an ellipsis, and how long it has worked,
// last in the conversation. A turn that has worked a while gets a tip under it in the main
// window's panel, kept for the turn in every window; another session's turn soon after gets
// none. A new word comes round in time. The line goes once the turn ends, and for a session gone
// silent mid-turn, which its header speaks for.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/working-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.tips.working');
  var started = Date.now() - 60e3;
  var picked = [];
  var shape = {};
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    return cmd === 'get_board' ? p.then(shaped) : p;
  };
  var shaped = function (board) {
    board.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        var at = picked.findIndex(function (p) { return p.key === s.key; });
        // Two whose conversations can be read.
        if (at < 0 && picked.length < 2 && !l.hidden && !l.device && !s.visitor && !s.archived && (s.harness === 'claude' || s.harness === 'codex')) {
          picked.push({ key: s.key, lane: l.id });
          at = picked.length - 1;
        }
        if (at < 0) {
          if (s.status === 'working' || s.status === 'needs') s.status = 'idle';
          return;
        }
        var own = shape[s.key] || {};
        s.status = own.status || 'working';
        s.statusSince = started + at * 30e3;
        s.lastPromptAt = started + at * 30e3;
        s.stalledSince = own.stalledSince || null;
        s.snoozedUntil = undefined;
        s.queued = undefined;
        s.asks = false;
        s.lastActivity = Date.now();
      });
    });
    return board;
  };
  var push = function () { return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); }); };
  var row = function (i) { return picked[i] && document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var open = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    row(i).click();
  };
  var line = function () { return q('.ca-sessinsp .ca-thread .ca-working'); };
  var word = function () { var w = q('.ca-sessinsp .ca-working-word'); return w ? w.textContent : ''; };
  var tip = function () { return q('.ca-sessinsp .ca-working-tip'); };
  // The tip the first turn got, as kept for every window.
  var claim = function () {
    try {
      return (JSON.parse(localStorage.getItem('codealong.tips.working') || '[]') || []).filter(function (c) { return c.turn === picked[0].key + ':' + started; })[0] || null;
    } catch (e) { return null; }
  };

  say('working check: waiting for the list');
  until(function () { return row(0) && row(1); }, 20000, 'two sessions to work')
    .then(function () {
      open(0);
      return until(function () { return line() && tip(); }, 10000, 'the working line and its tip');
    })
    .then(function () {
      var l = line();
      check('the mark breathes beside it', !!l.querySelector('.ca-lmark[data-status="working"]'));
      var gray = document.createElement('span');
      gray.style.color = 'var(--ink-3)';
      l.appendChild(gray);
      var fill = getComputedStyle(l.querySelector('.ca-lmark-c')).fill;
      check('quiet, its C gray', !!l.querySelector('.ca-lmark.is-quiet') && fill === getComputedStyle(gray).color, fill + ' vs ' + getComputedStyle(gray).color);
      gray.remove();
      var arc = l.querySelector('.ca-lmark-arc i');
      var glide = getComputedStyle(arc).animationName;
      check('its color gliding round it', glide === 'ca-mark-glide', glide);
      // A Run played on it (as mark.ts plays one on the bar's) takes the glide's place while it plays.
      var mark = l.querySelector('.ca-lmark');
      mark.classList.add('is-run');
      var run = getComputedStyle(arc).animationName;
      mark.classList.remove('is-run');
      check('a Run plays over the glide', run === 'ca-mark-run' && getComputedStyle(arc).animationName === 'ca-mark-glide', run);
      check('a word with an ellipsis', /^[A-Z][a-z].*…$/.test(word()), word());
      var took = q('.ca-sessinsp .ca-working-for');
      check('and how long it has worked', took && took.textContent === '1m', took && took.textContent);
      var thread = q('.ca-sessinsp .ca-thread');
      check('last in the conversation', thread.lastElementChild === l, thread.lastElementChild && thread.lastElementChild.className);
      check('a tip under it, as a tip', /^Tip\s/.test(tip().textContent), tip().textContent);
      var c = claim();
      check('kept for the turn', !!c && c.id === tip().getAttribute('data-tip'), JSON.stringify(c));
      var first = word();
      return wait(31e3).then(function () {
        check('another word in time', word() !== first && /…$/.test(word()), first + ' → ' + word());
      });
    })
    .then(function () {
      var kept = claim();
      open(1);
      var title = function () { var p = q('.ca-sessinsp'); return p ? p.getAttribute('aria-label') : ''; };
      var was = title();
      return until(function () { return title() !== was && line(); }, 5000, 'the second session’s line')
        // A beat of the line's clock, when a tip would come.
        .then(function () { return wait(11e3); })
        .then(function () {
          check('another turn soon after gets none', !!line() && !tip(), tip() ? tip().textContent : 'none');
          open(0);
          return until(function () { return tip(); }, 5000, 'the first turn’s tip again');
        })
        .then(function () {
          check('the first keeps the one it got', tip().getAttribute('data-tip') === kept.id, tip().getAttribute('data-tip') + ' vs ' + kept.id);
        });
    })
    .then(function () {
      shape[picked[0].key] = { stalledSince: Date.now() - 600e3 };
      return push().then(function () { return until(function () { return !line(); }, 5000, 'the line to go for a silent session'); });
    })
    .then(function () {
      check('gone for a session gone silent', !line());
      shape[picked[0].key] = { status: 'idle' };
      return push().then(function () { return wait(500); });
    })
    .then(function () {
      check('and once its turn ends', !line());
    })
    .then(done, stopped);
})();
