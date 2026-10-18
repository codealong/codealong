// Checks the line at the end of a conversation while its agent works (Working.tsx, working.ts):
// the mark quiet (its C gray) and still, turning as the bar's does once pointed at, a word for
// what it's up to with an ellipsis whose dots light in turn, and how long it has worked, held still,
// last in the conversation. A turn that has worked a while gets a tip on its right in the main
// window's panel, its first line level with the word, or under it once the line has no room, kept for the turn in every window; another session's turn soon after gets
// none. A new word comes round in time. The line goes once the turn ends, and for a session gone
// silent mid-turn, which its header speaks for. One whose turn is over that waits on a command it
// left running in the background says what it waits on in the line's place, with its clock, or
// that it waits on a command when the command couldn't be read.
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
        s.background = own.background;
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
  var COMMAND = 'until gh pr checks 139 | grep -q .;\n  do sleep 10; done; gh pr checks 139 --watch --interval 15 2>&1 | tail -4';
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
      check('the mark beside it', !!l.querySelector('.ca-lmark[data-status="working"]'));
      var gray = document.createElement('span');
      gray.style.color = 'var(--ink-3)';
      l.appendChild(gray);
      var fill = getComputedStyle(l.querySelector('.ca-lmark-c')).fill;
      check('quiet, its C gray', !!l.querySelector('.ca-lmark.is-quiet') && fill === getComputedStyle(gray).color, fill + ' vs ' + getComputedStyle(gray).color);
      gray.remove();
      var moving = l.querySelector('.ca-lmark').getAnimations({ subtree: true });
      check('and still', moving.length === 0, moving.map(function (a) { return a.animationName || a.transitionProperty; }).join(', '));
      // Pointed at, it turns as the bar's mark does: the line's rules for pointing, with a class
      // for :hover, which a page can't put on, and its turn played to the end.
      var copies = [];
      Array.prototype.forEach.call(document.styleSheets, function (sheet) {
        var rules;
        try { rules = sheet.cssRules; } catch (e) { return; }
        Array.prototype.forEach.call(rules, function (r) {
          if (r.selectorText && r.selectorText.indexOf('.ca-working-head:hover') >= 0) copies.push(r.cssText.split('.ca-working-head:hover').join('.ca-working-head.is-pointed'));
        });
      });
      var pointing = document.createElement('style');
      pointing.textContent = copies.join('\n');
      document.head.appendChild(pointing);
      var lead = l.querySelector('.ca-working-head');
      lead.classList.add('is-pointed');
      var c = l.querySelector('.ca-lmark-c');
      var pop = l.querySelector('.ca-lmark-pop');
      [c, pop].forEach(function (e) { e.getAnimations().forEach(function (a) { a.finish(); }); });
      var turn = getComputedStyle(c).rotate + ' · ' + getComputedStyle(pop).scale;
      lead.classList.remove('is-pointed');
      pointing.remove();
      check('pointed at, it turns as the bar’s mark does', turn === '360deg · 1.15', copies.length + ' rules · ' + turn);
      check('a word with an ellipsis', /^[A-Z][a-z].*\.\.\.$/.test(word()), word());
      // Its dots light in turn, and the time after them holds still.
      var took = q('.ca-sessinsp .ca-working-for');
      var dots = Array.prototype.slice.call(l.querySelectorAll('.ca-working-dots > span'));
      var at = function (t) {
        dots.forEach(function (d) { d.getAnimations().forEach(function (a) { a.pause(); a.currentTime = t; }); });
        var lit = dots.map(function (d) { return Number(getComputedStyle(d).opacity); });
        return { lit: lit.indexOf(Math.max.apply(null, lit)), left: took.getBoundingClientRect().left };
      };
      var beats = [400, 600, 800, 1400].map(at);
      dots.forEach(function (d) { d.getAnimations().forEach(function (a) { a.play(); }); });
      var still = beats.every(function (b) { return b.left === beats[0].left; });
      check('its dots lighting in turn, the time holding still', beats.slice(0, 3).map(function (b) { return b.lit; }).join(' ') === '0 1 2' && still,
        beats.map(function (b) { return b.lit + '@' + b.left; }).join(' '));
      check('and how long it has worked', took && took.textContent === '1m', took && took.textContent);
      var thread = q('.ca-sessinsp .ca-thread');
      check('last in the conversation', thread.lastElementChild === l, thread.lastElementChild && thread.lastElementChild.className);
      check('a tip, as a tip', /^Tip\s/.test(tip().textContent), tip().textContent);
      // Its words end at the line's right edge, and start beside the word or under it.
      var head = l.querySelector('.ca-working-head').getBoundingClientRect();
      var box = tip().getBoundingClientRect();
      var words = document.createRange();
      words.selectNodeContents(tip());
      var lines = Array.prototype.slice.call(words.getClientRects());
      var right = Math.max.apply(null, lines.map(function (r) { return r.right; }));
      var left = Math.min.apply(null, lines.map(function (r) { return r.left; }));
      var beside = Math.abs(box.top - head.top) < 1 && left > head.right;
      check('on the line’s right', Math.abs(box.right - l.getBoundingClientRect().right) < 1 && Math.abs(right - box.right) < 2 && (beside || box.top >= head.bottom),
        (beside ? 'beside' : 'under') + ' · ends ' + Math.round(right) + ' of ' + Math.round(box.right) + ' · ' + lines.length + ' rects');
      var c = claim();
      check('kept for the turn', !!c && c.id === tip().getAttribute('data-tip'), JSON.stringify(c));
      var first = word();
      // The next word comes long enough to leave the tip no room beside it: the line grows, and a
      // conversation read at its end stays there.
      q('.ca-sessinsp .ca-working-word').setAttribute('data-was', '');
      var long = document.createElement('style');
      long.textContent = '.ca-sessinsp .ca-thread { max-height: 200px; } .ca-sessinsp .ca-working-word:not([data-was]) { display: inline-block; min-width: ' + (l.clientWidth - 100) + 'px; }';
      document.head.appendChild(long);
      var tall = l.getBoundingClientRect().height;
      return wait(300)
        .then(function () {
          thread.scrollTop = thread.scrollHeight;
          return wait(31e3);
        })
        .then(function () {
          check('another word in time', word() !== first && /\.\.\.$/.test(word()), first + ' → ' + word());
          var off = thread.scrollHeight - thread.scrollTop - thread.clientHeight;
          var under = tip().getBoundingClientRect().top >= l.querySelector('.ca-working-head').getBoundingClientRect().bottom;
          check('a longer word puts the tip under it, the view kept at the end', under && line().getBoundingClientRect().height > tall && thread.scrollHeight > thread.clientHeight && off < 1,
            (under ? 'under' : 'beside') + ' · ' + Math.round(tall) + ' → ' + Math.round(line().getBoundingClientRect().height) + 'px · ' + Math.round(off) + 'px from the end');
          long.remove();
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
      check('with its clock, not the mark or a tip', !!w.querySelector('.ca-glyph--waiting') && !w.querySelector('.ca-lmark') && !tip());
      check('last in the conversation', q('.ca-sessinsp .ca-thread').lastElementChild === w);
      shape[picked[0].key] = { background: '' };
      return push().then(function () { return until(function () { return waiting() && !waiting().querySelector('code'); }, 5000, 'the waiting line without a command'); });
    })
    .then(function () {
      check('or that it waits on a command, when it couldn’t be read', waiting().textContent === 'Waiting on a command in the background', waiting().textContent);
      var narrow = fitsNarrow();
      check('which wraps in a narrow panel rather than going past its side', narrow === true, narrow);
      shape[picked[0].key] = { status: 'idle' };
      return push().then(function () { return until(function () { return !waiting(); }, 5000, 'the waiting line to go'); });
    })
    .then(function () {
      check('and once its turn ends, neither', !line() && !waiting());
    })
    .then(done, stopped);
})();
