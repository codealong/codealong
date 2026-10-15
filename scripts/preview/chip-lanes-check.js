// Checks the lanes of chips on the lists' rows (LaneRow.tsx, PrsView.tsx, TodoView.tsx, and
// .ca-chips in src/styles/chips.css): every chip is one height and shape, and none is cut off,
// each showing whole or not at all. In Sessions it also checks that a session's account is on
// the second line, not the first; that Rows in the bar over the list puts each session on one
// line and remembers it, and that a point on a one-line row finds that row, in a board that's a
// stacking context of its own; and in every list that a right-click (or ⌃-click) opens a row's
// menu without selecting it.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh todo '' scripts/preview/chip-lanes-check.js
// PREVIEW_SIZE=1290,800 checks a narrow list, where the lanes are shortest.
(function () {
  var CHIPS = '.ca-chip, .ca-pr, .ca-tag, .ca-pill, .ca-labelchip';
  var shown = function (el) { var r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };

  // A lane's pieces: on its line and inside it, or wrapped onto the line that's cut off.
  var lanes = function () {
    var cut = [];
    var seen = 0;
    all('.ca-chips, .ca-prrow-fit').filter(shown).forEach(function (lane) {
      var box = lane.getBoundingClientRect();
      // The lane's padding is room for a focus ring, not for chips.
      var right = box.right - 4;
      var top = box.top + 4;
      Array.prototype.forEach.call(lane.children, function (c) {
        var r = c.getBoundingClientRect();
        if (r.width === 0) return;
        seen++;
        var onLine = r.top < top + 10;
        if (onLine && r.right > right + 0.5) cut.push((c.textContent || c.className).slice(0, 24) + ' +' + Math.round(r.right - right));
        if (!onLine && r.top < box.bottom) cut.push((c.textContent || c.className).slice(0, 24) + ' peeks');
      });
    });
    return { seen: seen, cut: cut };
  };
  // A chip that shortens does it inside itself: nothing of it spills past its own edge.
  var spills = function () {
    return all('.ca-wtchip, .ca-todochip').filter(shown).filter(function (c) {
      return getComputedStyle(c).overflow === 'visible' && c.scrollWidth > c.clientWidth + 1;
    }).map(function (c) { return c.textContent.slice(0, 20); });
  };
  var shapes = function () {
    var odd = [];
    all('[role="option"], .ca-lane').forEach(function (row) {
      all(CHIPS, row).filter(shown).forEach(function (c) {
        var s = getComputedStyle(c);
        var h = Math.round(c.getBoundingClientRect().height);
        if (h !== 20 || parseFloat(s.borderTopLeftRadius) < 10 || s.fontSize !== '11px') odd.push((c.textContent || '').slice(0, 16) + ' ' + h + 'px/' + s.borderTopLeftRadius + '/' + s.fontSize);
      });
    });
    return odd;
  };
  var report = function (name) {
    var l = lanes();
    check(name + ': no chip is cut off', l.seen > 0 && l.cut.length === 0, l.seen + ' chips' + (l.cut.length ? ', ' + l.cut.slice(0, 4).join(' | ') : ''));
    var sp = spills();
    check(name + ': a long name shortens inside its chip', sp.length === 0, sp.slice(0, 4).join(' | '));
    var odd = shapes();
    check(name + ': every chip is one height, shape and text size', odd.length === 0, odd.slice(0, 4).join(' | '));
  };

  var fire = function (el, type, init) { el.dispatchEvent(new MouseEvent(type, Object.assign({ bubbles: true, cancelable: true }, init))); };
  var isSelected = function (row) { return row.classList.contains('is-selected') || row.getAttribute('aria-selected') === 'true'; };
  var selectedId = function () { var s = all('[role="option"]').filter(isSelected)[0]; return s ? s.id : ''; };
  // A right-click, then a ⌃-click, on a row that isn't selected: its menu, and no selection.
  var rightClicks = function () {
    var row = all('[role="option"]').filter(function (r) { return shown(r) && !isSelected(r); })[0];
    if (!row) {
      note('right-click: no row to try, skipped');
      return;
    }
    var before = selectedId();
    var n = menus();
    fire(row, 'mousedown', { button: 2 });
    fire(row, 'contextmenu', { button: 2 });
    return wait(300).then(function () {
      check('a right-click opens the row’s menu', menus() === n + 1);
      var kept = !isSelected(row) && selectedId() === before;
      check('and leaves the selection alone', kept, kept ? '' : selectedId() + ' was ' + before);
      n = menus();
      fire(row, 'mousedown', { button: 0, ctrlKey: true });
      fire(row, 'contextmenu', { button: 0, ctrlKey: true });
      return wait(300);
    }).then(function () {
      check('a ⌃-click does the same', menus() === n + 1 && !isSelected(row) && selectedId() === before);
      check('no ring is left on the row once its menu closes', !document.querySelector('[data-menu]'));
      fire(row, 'mousedown', { button: 0 });
      return wait(300);
    }).then(function () {
      check('a click still selects it', isSelected(row));
    });
  };

  var rowsButton = function (label) { return document.querySelector('.ca-sessionbar button[aria-label="' + label + '"]'); };
  var sessionRows = function () { return all('.ca-sessionlist .ca-lane.is-agent').filter(shown); };
  var sessions = function () {
    var rows = sessionRows();
    var account = function (row) { return all('.ca-chip', row).filter(function (c) { return /Double-click to open in/.test(c.title); })[0]; };
    check('a session’s first line has no chip but what it waits on you for', rows.every(function (r) { return all('.ca-agentrow-line1 .ca-chip', r).every(function (c) { return c.classList.contains('ca-insight'); }); }));
    check('its account is on the second line, after its worktree', rows.every(function (r) {
      var a = account(r);
      var lane = r.querySelector('.ca-agentrow-chips');
      return a && lane && a.parentElement === lane && lane.firstElementChild.classList.contains('ca-wtchip') && lane.lastElementChild === a;
    }));
    check('Rows starts on two lines', rowsButton('Two lines') && rowsButton('Two lines').classList.contains('is-on'));
    var tall = rows[0].getBoundingClientRect().height;
    rowsButton('One line').click();
    return until(function () { return sessionRows().length > 0 && sessionRows().every(function (r) { return r.classList.contains('is-slim'); }); }, 3000)
      .then(function () { return wait(200); })
      .then(function () {
        var slim = sessionRows();
        var heights = slim.map(function (r) { return Math.round(r.getBoundingClientRect().height); });
        // A row's hairline is part of its height.
        check('One line: each session is one line high', heights.every(function (h) { return h <= 34; }) && tall > 50, 'from ' + Math.round(tall) + ' to ' + Math.max.apply(null, heights));
        check('and it’s remembered', localStorage.getItem('codealong.sessionRows') === 'one');
        check('with no account chip, and its account on its status', slim.every(function (r) { return !account(r) && /·/.test(r.querySelector('.ca-seg-glyph').title); }));
        // A chip that wrapped away is still in the page, below its lane's one line.
        var onLine = function (r) {
          var lane = r.querySelector('.ca-agentline-tags');
          return lane ? all('.ca-chip', lane).filter(function (c) { return c.getBoundingClientRect().top < lane.getBoundingClientRect().top + 14; }) : [];
        };
        var titles = slim.filter(function (r) { var t = r.querySelector('.ca-agentline-title'); return t.scrollWidth > t.clientWidth + 1 && onLine(r).length > 0; });
        check('a title gives way only after the chips have', titles.length === 0, titles.length + ' rows');
        var leftAligned = slim.filter(function (r) {
          var chips = onLine(r);
          if (chips.length === 0) return false;
          var lane = r.querySelector('.ca-agentline-tags').getBoundingClientRect();
          return lane.right - 4 - chips[chips.length - 1].getBoundingClientRect().right > 1;
        });
        check('the chips sit at the right end of the line', leftAligned.length === 0, leftAligned.length + ' rows');
        // Rows hit where they're drawn: the board is a stacking context, so WebKit draws its
        // positioned rows in its scroller's layer, not each in a layer of its own (App.css).
        var board = document.querySelector('.ca-board');
        check('the board is a stacking context of its own', board && getComputedStyle(board).isolation === 'isolate');
        var hits = 0;
        var off = [];
        slim.forEach(function (r) {
          var b = r.getBoundingClientRect();
          if (b.top < 0 || b.bottom > innerHeight) return;
          [b.top + 2, (b.top + b.bottom) / 2, b.bottom - 2].forEach(function (y) {
            var el = document.elementFromPoint((b.left + b.right) / 2, y);
            var row = el && el.closest('.ca-lane');
            if (!row) return;
            hits++;
            if (row !== r) off.push(r.querySelector('.ca-agentline-title').textContent.slice(0, 20) + ' @' + Math.round(y - b.top));
          });
        });
        check('a point in a row finds that row', hits > 0 && off.length === 0, hits + ' points' + (off.length ? ', ' + off.slice(0, 4).join(' | ') : ''));
        report('one line');
        return rightClicks();
      })
      .then(function () {
        rowsButton('Two lines').click();
        return until(function () { return localStorage.getItem('codealong.sessionRows') === 'two'; }, 3000);
      });
  };

  say('chip lanes check: waiting for the list');
  until(function () { return document.querySelector('[role="option"]'); }, 20000)
    .then(function () { return wait(800); })
    .then(function () {
      var label = document.querySelector('[role="listbox"]').getAttribute('aria-label');
      report(label);
      check('Rows shows only in Sessions', !!document.querySelector('.ca-sessionbar') === (label === 'Sessions'));
      return label === 'Sessions' ? sessions() : rightClicks();
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
