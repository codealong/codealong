// Checks the line at the end of a conversation while its agent works (Working.tsx, working.ts):
// a small gray square tumbling, the only thing that moves, with no word beside it, and how long it
// has worked, held still, last in the conversation. A turn that has worked a while gets a tip on
// its right in the main window's panel, its first line level with the time, or under it once the
// line has no room, kept for the turn in every window; a time that grows wider and leaves the tip
// no room puts it under, the view kept at the conversation's end. Another session's turn soon
// after gets none. The line goes once the turn ends, and for a session gone
// silent mid-turn, which its header speaks for. One whose turn is over that waits on a command it
// left running in the background says what it waits on in the line's place, with its clock, or
// that it waits on a command when the command couldn't be read. One that waits on a command you
// allowed in your terminal says where that runs and what, whether its turn is over or not.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/working-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.tips.working');
  // A turn of 1m40s, which says 2m soon after the check has looked at it.
  var started = Date.now() - 100e3;
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
        s.background = own.background;
        s.running = own.running;
        s.snoozedUntil = undefined;
        s.raised = undefined;
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
  var COMMAND = 'until gh pr checks 139 | grep -q .;\n  do sleep 10; done; gh pr checks 139 --watch --interval 15 2>&1 | tail -4';
  var RUN = { terminal: 'Rex', command: 'node scripts/publish.mjs' };
  // Whether the conversation, its panel made narrow, has nothing that goes past its side.
  var fitsNarrow = function () {
    var panel = q('.ca-sessinsp');
    var thread = q('.ca-sessinsp .ca-thread');
    var was = panel.style.cssText;
    panel.style.cssText = was + ';width:200px;min-width:0;max-width:200px;flex:none';
    var fits = thread.scrollWidth <= thread.clientWidth + 1;
    var said = thread.scrollWidth + ' in ' + thread.clientWidth;
    panel.style.cssText = was;
    return fits || said;
  };
  var line = function () { return q('.ca-sessinsp .ca-thread .ca-working:not(.is-waiting)'); };
  var waiting = function () { return q('.ca-sessinsp .ca-thread .ca-working.is-waiting'); };
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
      var square = l.querySelector('.ca-working-head > .ca-working-tumble > i');
      check('a square at its head, said as working', !!square && square.parentElement.getAttribute('aria-label') === 'Working');
      var gray = document.createElement('span');
      gray.style.color = 'var(--ink-2)';
      l.appendChild(gray);
      var fill = getComputedStyle(square).backgroundColor;
      check('in the gray of the text', fill === getComputedStyle(gray).color, fill + ' vs ' + getComputedStyle(gray).color);
      gray.remove();
      check('with no word beside it', !l.querySelector('.ca-working-word'), word());
      // Only the square moves: top over bottom by half its round (its y flipped), then side to
      // side (its x flipped too), and the time after it holds still throughout.
      var took = q('.ca-sessinsp .ca-working-for');
      var moving = l.querySelector('.ca-working-head').getAnimations({ subtree: true });
      var at = function (t) {
        moving.forEach(function (a) { a.pause(); a.currentTime = t; });
        var m = new DOMMatrix(getComputedStyle(square).transform);
        return { pose: Math.round(m.m11) + ',' + Math.round(m.m22), left: took.getBoundingClientRect().left };
      };
      var poses = [0, 800, 1590].map(at);
      moving.forEach(function (a) { a.play(); });
      var still = poses.every(function (p) { return p.left === poses[0].left; });
      check('only the square moving, top over bottom, then side to side, the time holding still',
        moving.length === 1 && moving[0].animationName === 'ca-working-tumble' && moving[0].effect.target === square && poses.map(function (p) { return p.pose; }).join(' ') === '1,1 1,-1 -1,-1' && still,
        moving.map(function (a) { return a.animationName || a.transitionProperty; }).join(', ') + ' · ' + poses.map(function (p) { return p.pose + '@' + p.left; }).join(' '));
      check('and how long it has worked', took && /^[12]m$/.test(took.textContent), took && took.textContent);
      var thread = q('.ca-sessinsp .ca-thread');
      check('last in the conversation', thread.lastElementChild === l, thread.lastElementChild && thread.lastElementChild.className);
      check('a tip, as a tip', /^Tip\s/.test(tip().textContent), tip().textContent);
      // Its words end at the line's right edge, and start beside the time, on one line, or under it.
      var head = l.querySelector('.ca-working-head').getBoundingClientRect();
      var box = tip().getBoundingClientRect();
      var words = document.createRange();
      words.selectNodeContents(tip());
      var lines = Array.prototype.slice.call(words.getClientRects());
      var right = Math.max.apply(null, lines.map(function (r) { return r.right; }));
      var left = Math.min.apply(null, lines.map(function (r) { return r.left; }));
      var beside = Math.abs(box.top - head.top) < 1 && left > head.right;
      check('on the line’s right', Math.abs(box.right - l.getBoundingClientRect().right) < 1 && Math.abs(right - box.right) < 2 && (beside ? box.height < 1.5 * head.height : box.top >= head.bottom),
        (beside ? 'beside' : 'under') + ' · ends ' + Math.round(right) + ' of ' + Math.round(box.right) + ' · ' + Math.round(box.height) + 'px high');
      var c = claim();
      check('kept for the turn', !!c && c.id === tip().getAttribute('data-tip'), JSON.stringify(c));
      // The tip, cut to a short line, sits beside the time; the time grows wide enough to leave it
      // no room there (as 9m becoming 10m does, made wider here), and once it says the next minute
      // the line has grown and a conversation read at its end stays there.
      var short = document.createElement('style');
      short.textContent = '.ca-sessinsp .ca-thread { max-height: 200px; } .ca-sessinsp .ca-working-tip { max-width: 12em; white-space: nowrap; overflow: hidden; }';
      document.head.appendChild(short);
      var tall;
      var was;
      var offBefore;
      return wait(300)
        .then(function () {
          thread.scrollTop = thread.scrollHeight;
          tall = l.getBoundingClientRect().height;
          was = took.textContent;
          took.style.display = 'inline-block';
          took.style.minWidth = l.clientWidth - 100 + 'px';
          return wait(100);
        })
        .then(function () {
          offBefore = thread.scrollHeight - thread.scrollTop - thread.clientHeight;
          return until(function () { return took.textContent !== was; }, 75000, 'the time to say the next minute');
        })
        .then(function () { return wait(100); })
        .then(function () {
          var off = thread.scrollHeight - thread.scrollTop - thread.clientHeight;
          var under = tip().getBoundingClientRect().top >= l.querySelector('.ca-working-head').getBoundingClientRect().bottom;
          check('a time grown wider puts the tip under it, the view kept at the end', under && line().getBoundingClientRect().height > tall && thread.scrollHeight > thread.clientHeight && off < 1,
            was + ' → ' + took.textContent + ' · ' + (under ? 'under' : 'beside') + ' · ' + Math.round(tall) + ' → ' + Math.round(line().getBoundingClientRect().height) + 'px · ' + Math.round(offBefore) + ' then ' + Math.round(off) + 'px from the end');
          took.style.display = '';
          took.style.minWidth = '';
          short.remove();
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
      check('gone for a session gone silent', !line() && !waiting());
      // At work again, then its turn over with a command left running.
      shape[picked[0].key] = {};
      return push().then(function () { return until(function () { return line(); }, 5000, 'the line back for a session at work again'); });
    })
    .then(function () {
      shape[picked[0].key] = { background: COMMAND };
      return push().then(function () { return until(function () { return !line() && waiting(); }, 5000, 'the waiting line in the working line’s place'); });
    })
    .then(function () {
      var w = waiting();
      var cmd = w.querySelector('code');
      check('one waiting on a command in the background says what it waits on', /^Waiting on/.test(w.textContent) && !!cmd && cmd.textContent === 'until gh pr checks 139 | grep -q .; do sleep 10; done; gh pr checks 139 --watch --interval 15 2>&1 | tail -4', w.textContent);
      var said = w.querySelector('.ca-working-word');
      var oneLine = cmd.getClientRects().length === 1 && said.getClientRects().length === 1 && said.getBoundingClientRect().height < 20 && Math.abs(said.getBoundingClientRect().top - cmd.getBoundingClientRect().top) < 6;
      check('on one line, all of it on hover as it was written', oneLine && cmd.getAttribute('title') === COMMAND, said.getBoundingClientRect().height + 'px words');
      var narrow = fitsNarrow();
      check('fitting a narrow panel', narrow === true, narrow);
      check('with its clock, not the square or a tip', !!w.querySelector('.ca-glyph--waiting') && !w.querySelector('.ca-working-tumble') && !tip());
      check('last in the conversation', q('.ca-sessinsp .ca-thread').lastElementChild === w);
      shape[picked[0].key] = { background: '' };
      return push().then(function () { return until(function () { return waiting() && !waiting().querySelector('code'); }, 5000, 'the waiting line without a command'); });
    })
    .then(function () {
      check('or that it waits on a command, when it couldn’t be read', waiting().textContent === 'Waiting on a command in the background', waiting().textContent);
      var narrow = fitsNarrow();
      check('which wraps in a narrow panel rather than going past its side', narrow === true, narrow);
      // Its command in the background waits on a run you allowed in Rex.
      shape[picked[0].key] = { background: 'codealong terminal run "cd /Users/me/projects/northwind-app && node scripts/publish.mjs" 2>&1 | tail -60', running: RUN };
      return push().then(function () { return until(function () { return waiting() && /^Running in/.test(waiting().textContent); }, 5000, 'the line saying where the run is'); });
    })
    .then(function () {
      var w = waiting();
      var said = w.querySelector('.ca-working-word');
      var cmd = w.querySelector('code');
      check('one waiting on a command you allowed in your terminal says where it runs and what', said.textContent === 'Running in Rex' && !!cmd && cmd.textContent === RUN.command && cmd.getAttribute('title') === RUN.command, w.textContent);
      var oneLine = cmd.getClientRects().length === 1 && said.getClientRects().length === 1 && Math.abs(said.getBoundingClientRect().top - cmd.getBoundingClientRect().top) < 6;
      check('on one line, with its clock', oneLine && !!w.querySelector('.ca-glyph--waiting') && !tip());
      var narrow = fitsNarrow();
      check('fitting a narrow panel too', narrow === true, narrow);
      // Waiting on it in its turn, its command not in the background.
      shape[picked[0].key] = { running: RUN };
      return push().then(function () { return until(function () { return waiting() && !line(); }, 5000, 'the run in the working line’s place mid-turn'); });
    })
    .then(function () {
      check('and mid-turn, in the working line’s place', /^Running in Rex/.test(waiting().textContent) && !line(), waiting().textContent);
      shape[picked[0].key] = { status: 'idle' };
      return push().then(function () { return until(function () { return !waiting(); }, 5000, 'the waiting line to go'); });
    })
    .then(function () {
      check('and once its turn ends, neither', !line() && !waiting());
    })
    .then(done, stopped);
})();
