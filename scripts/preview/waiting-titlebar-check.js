// Checks the waiting window's title bar (waiting.rs, SessionPanel.tsx `Hotkeys.titleBar`): the
// window has none of its own, so the strip along the top of the card is one. It drags the window,
// its first line starts clear of the window's buttons (macOS 26 puts them 9 to 69 points in) and
// is centered on them (16 down), at actual size and at a zoom, which the window's buttons don't
// take; and narrow, what it says and its buttons stay beside the window's buttons, on their one
// line level with the window's, what it hasn't room for in its ⋯ menu rather than run past its
// end. With a PR shown its card's strip is one too (the second run). The window's buttons
// themselves are native, so not here.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-titlebar-check.js
//   PREVIEW_WAIT=60000 PREVIEW_INIT=scripts/preview/waiting-review-init.js scripts/preview.sh waiting '' scripts/preview/waiting-titlebar-check.js
(function () {
  var latest = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.zoom = 100;
      latest = board;
      return board;
    });
  };

  var strip = function () { return q('.ca-waiting .ca-reply > .ca-reply-queue'); };
  var note = function () { return q('.ca-reply-queue-note', strip()); };
  /** Where the strip's first line is, in the window's points: where the note starts, and its
   *  middle, which the line's is (it's centered in it). */
  var firstLine = function () {
    var r = note().getBoundingClientRect();
    return { left: r.left, middle: r.top + r.height / 2 };
  };
  var near = function (a, b, by) { return Math.abs(a - b) <= by; };
  var at = function (l) { return 'starts at ' + l.left.toFixed(1) + ', its middle ' + l.middle.toFixed(1) + ' down'; };

  say('waiting title bar check: waiting for the card');
  until(function () { return !!strip() && !!note() && !!latest; }, 30000, 'the card’s strip')
    .then(function () { return wait(500); })
    .then(function () {
      var s = strip();
      check('the strip is the window’s title bar', s.classList.contains('is-titlebar'), s.className);
      check('and drags the window from anywhere but its buttons', s.getAttribute('data-tauri-drag-region') === 'deep', String(s.getAttribute('data-tauri-drag-region')));
      check('it’s at the top of the window', near(s.getBoundingClientRect().top, 0, 0.5), 'top ' + s.getBoundingClientRect().top);
      var l = firstLine();
      check('its first line starts clear of the window’s buttons', near(l.left, 80, 1), at(l));
      check('and is centered on them', near(l.middle, 16, 1), at(l));
      var buttons = all('.ca-hotkey', s);
      check('its buttons are on that line at this width', buttons.length > 0 && buttons.every(function (b) { var r = b.getBoundingClientRect(); return near(r.top + r.height / 2, 16, 1); }), buttons.length + ' buttons');
      // At a zoom the card draws bigger, and the window's buttons don't.
      window.__emit('board', Object.assign({}, latest, { zoom: 125, generatedAt: latest.generatedAt + 1 }));
      return until(function () { return getComputedStyle(q('.ca-waiting .ca-reply')).zoom === '1.25'; }, 5000, 'the card at 125%');
    })
    .then(function () { return wait(300); })
    .then(function () {
      var l = firstLine();
      check('at 125% the first line still starts clear of them', near(l.left, 80, 1.5), at(l));
      check('and is still centered on them', near(l.middle, 16, 1.5), at(l));
      window.__emit('board', Object.assign({}, latest, { zoom: 100, generatedAt: latest.generatedAt + 2 }));
      return until(function () { return getComputedStyle(q('.ca-waiting .ca-reply')).zoom === '1'; }, 5000, 'the card at actual size');
    })
    .then(function () {
      // Narrow, what it says and its buttons stay beside the window's buttons, and the buttons'
      // first line level with them.
      var root = q('.ca-waiting');
      root.style.width = '520px';
      return wait(300).then(function () {
        var s = strip();
        var first = all('.ca-hotkey', s)[0].getBoundingClientRect();
        var block = q('.ca-hotkeys', s).getBoundingClientRect();
        var l = firstLine();
        check('narrow, the buttons stay beside the window’s', block.left >= l.left - 1, 'the buttons start ' + block.left.toFixed(1) + ' in');
        check('their first line level with them', near(first.top + first.height / 2, 16, 1), 'first button’s middle ' + (first.top + first.height / 2).toFixed(1) + ' down');
        check('and the strip’s line stays clear of the window’s buttons, centered on them', near(l.left, 80, 1) && near(l.middle, 16, 1), at(l));
        var said = q('.ca-reply-queue-note', s).getBoundingClientRect();
        check('what it says keeps room for a word and its ellipsis to rest the pointer on', said.width >= 39, Math.round(said.width) + ' wide');
        check('nothing on the strip goes past its end', all('.ca-hotkey', s).every(function (b) { return b.getBoundingClientRect().right <= 520.5; }), 'last ends at ' + all('.ca-hotkey', s).pop().getBoundingClientRect().right.toFixed(1));
        root.style.width = '';
      });
    })
    .then(done, stopped);
})();
