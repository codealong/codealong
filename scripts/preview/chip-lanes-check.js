// Checks the lanes of chips on the lists' rows (LaneRow.tsx, PrsView.tsx with a PR's labels after
// its title, TodoView.tsx, and .ca-chips in src/styles/chips.css): every chip is one height and
// shape, and none is cut off, each showing whole or not at all. In Sessions, PRs and Todo it also
// checks that Rows in the bar over the list starts on one line and puts each row back on two and
// remembers it, both ways with no lines between a row's parts; that on one line a title gives way
// only after the chips, which sit at the line's end, and a point on a row finds that row, in a
// board that's a stacking context of its own; that a session's account is on its status on one
// line and on the second line on two, after its worktree, which grouped by worktree is left to
// the worktree's heading (whose chips are checked with the rest); and in every list that a
// right-click (or ⌃-click) opens a row's menu without selecting it.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 PREVIEW_GROUP=worktree scripts/preview.sh sessions '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/chip-lanes-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh todo '' scripts/preview/chip-lanes-check.js
// PREVIEW_SIZE=1290,800 checks a narrow list, where the lanes are shortest.
(function () {
  var CHIPS = '.ca-chip, .ca-pr, .ca-tag, .ca-pill, .ca-labelchip';
  var shown = function (el) { var r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };

  // What a lane lays out: its children, and theirs where a child lays out none of its own
  // (display: contents, as a PR's labels are on its one line).
  var pieces = function (el) {
    return Array.prototype.reduce.call(el.children, function (out, c) {
      return out.concat(getComputedStyle(c).display === 'contents' ? pieces(c) : [c]);
    }, []);
  };
  // A lane's pieces: on its line and inside it, or wrapped onto the line that's cut off. A PR's
  // pills and size at the end of its one line count as a lane too.
  var lanes = function () {
    var cut = [];
    var seen = 0;
    all('.ca-chips, .ca-prrow-fit, .ca-prrow .ca-prlabels, .ca-prrow-end').filter(shown).forEach(function (lane) {
      var box = lane.getBoundingClientRect();
      var st = getComputedStyle(lane);
      // The lane's padding is room for a focus ring, not for chips.
      var right = box.right - parseFloat(st.paddingRight);
      var top = box.top + parseFloat(st.paddingTop);
      // A lane on a row that doesn't fit its line is cut off whole.
      var line = lane.parentElement.getBoundingClientRect();
      if (lane.closest('.ca-lane') && right > line.right + 0.5) cut.push(lane.className + ' past its line +' + Math.round(right - line.right));
      pieces(lane).forEach(function (c) {
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

  // The lists with Rows, each with its rows and where it remembers the choice.
  var LISTS = {
    // A session's rows, not the folds that list more of them.
    Sessions: { rows: '.ca-sessionlist .ca-lane.is-agent:not(.is-fold)', key: 'codealong.sessionRows' },
    'Pull requests': { rows: '.ca-prrow', key: 'codealong.prRows' },
    Todos: { rows: '.ca-todorow', key: 'codealong.todoRows' },
  };
  var rowsButton = function (label) { return document.querySelector('.ca-listtools button[aria-label="' + label + '"]'); };
  // No line between a row's parts: its status, what it is and its age share one ground.
  var ruled = function (rows) {
    return rows.filter(function (r) {
      return all('.ca-seg-main, .ca-seg-age', r).some(function (seg) { return parseFloat(getComputedStyle(seg).borderLeftWidth) > 0; });
    });
  };
  var account = function (row) { return all('.ca-chip', row).filter(function (c) { return /Double-click to open in/.test(c.title); })[0]; };

  var oneLine = function (name, rows) {
    // A row a helper's or side conversation's is on one line either way.
    var heights = rows.map(function (r) { return Math.round(r.getBoundingClientRect().height); });
    // A row's hairline is part of its height.
    check(name + ': Rows starts on one line, each row one line high', rowsButton('One line') && rowsButton('One line').classList.contains('is-on') && rows.every(function (r) { return r.classList.contains('is-slim'); }) && heights.every(function (h) { return h <= 34; }), 'up to ' + Math.max.apply(null, heights));
    var lined = ruled(rows);
    check(name + ': no line between a row’s parts', lined.length === 0, lined.length + ' rows');
    // A chip that wrapped away is still in the page, below its lane's one line.
    var onLine = function (r) {
      var lane = r.querySelector('.ca-lane-tags');
      return lane ? all(CHIPS, lane).filter(function (c) { return shown(c) && c.getBoundingClientRect().top < lane.getBoundingClientRect().top + 14; }) : [];
    };
    var titles = rows.filter(function (r) { var t = r.querySelector('.ca-lane-headline'); return t && t.scrollWidth > t.clientWidth + 1 && onLine(r).length > 0; });
    check(name + ': a title gives way only after the chips have', titles.length === 0, titles.length + ' rows');
    var leftAligned = rows.filter(function (r) {
      var chips = onLine(r);
      if (chips.length === 0) return false;
      var lane = r.querySelector('.ca-lane-tags').getBoundingClientRect();
      return lane.right - 4 - chips[chips.length - 1].getBoundingClientRect().right > 1;
    });
    check(name + ': the chips sit at the right end of the line', leftAligned.length === 0, leftAligned.length + ' rows');
    var crampedAge = rows.filter(function (r) {
      var age = r.querySelector('.ca-seg-age');
      return age && age.scrollWidth > age.clientWidth + 1;
    });
    check(name + ': the age fits its column', crampedAge.length === 0, crampedAge.length + ' rows');
    // A chip that wrapped away still takes the keyboard, and focusing it scrolls nothing: a row's
    // line clips what doesn't fit, it doesn't scroll to it.
    var away = [];
    rows.forEach(function (r) {
      var lane = r.querySelector('.ca-lane-tags');
      if (!lane) return;
      var below = lane.getBoundingClientRect().top + 14;
      var hidden = all('button, [tabindex]', lane).filter(function (b) { return b.getBoundingClientRect().top >= below; })[0];
      if (hidden) away.push({ row: r, el: hidden });
    });
    var scrolled = away.slice(0, 20).filter(function (a) {
      a.el.focus();
      var moved = [a.row].concat(all('*', a.row)).filter(function (el) { return el.scrollTop !== 0 || el.scrollLeft !== 0; });
      a.el.blur();
      moved.forEach(function (el) { el.scrollTop = 0; el.scrollLeft = 0; });
      return moved.length > 0;
    });
    if (away.length === 0) note(name + ': no chip wrapped away to focus');
    else check(name + ': focusing a chip that wrapped away scrolls nothing', scrolled.length === 0, scrolled.length + ' of ' + Math.min(away.length, 20) + ' rows');
    if (name === 'Sessions') check('with no account chip, and its account on its status', rows.every(function (r) { return !account(r) && /·/.test(r.querySelector('.ca-seg-glyph').title); }));
    // Rows hit where they're drawn: the board is a stacking context, so WebKit draws its
    // positioned rows in its scroller's layer, not each in a layer of its own (App.css).
    var board = document.querySelector('.ca-board');
    check('the board is a stacking context of its own', board && getComputedStyle(board).isolation === 'isolate');
    var hits = 0;
    var off = [];
    rows.forEach(function (r) {
      var b = r.getBoundingClientRect();
      if (b.top < 0 || b.bottom > innerHeight) return;
      [b.top + 2, (b.top + b.bottom) / 2, b.bottom - 2].forEach(function (y) {
        var el = document.elementFromPoint((b.left + b.right) / 2, y);
        var row = el && el.closest('.ca-lane');
        if (!row) return;
        hits++;
        if (row !== r) off.push(r.querySelector('.ca-lane-headline').textContent.slice(0, 20) + ' @' + Math.round(y - b.top));
      });
    });
    check(name + ': a point in a row finds that row', hits > 0 && off.length === 0, hits + ' points' + (off.length ? ', ' + off.slice(0, 4).join(' | ') : ''));
  };

  var twoLines = function (name, rows) {
    var heights = rows.filter(function (r) { return !r.classList.contains('is-nested'); }).map(function (r) { return Math.round(r.getBoundingClientRect().height); });
    check(name + ': Two lines puts each row on two', heights.length > 0 && heights.every(function (h) { return h > 50; }), 'from ' + Math.min.apply(null, heights));
    check('and it’s remembered', localStorage.getItem(LISTS[name].key) === 'two');
    var lined = ruled(rows);
    check(name + ': still no line between a row’s parts', lined.length === 0, lined.length + ' rows');
    if (name !== 'Sessions') return;
    var tops = rows.filter(function (r) { return !r.classList.contains('is-nested'); });
    check('a session’s first line has no chip but what it waits on you for', tops.every(function (r) { return all('.ca-agentrow-line1 .ca-chip', r).every(function (c) { return c.classList.contains('ca-insight'); }); }));
    // Grouped by worktree, the heading says the worktree and its rows don't.
    var grouped = !!document.querySelector('.ca-grouphead.is-worktree-head');
    var under = tops.filter(function (r) { return !grouped || !r.closest('.ca-group') || !r.closest('.ca-group').querySelector('.is-worktree-head'); });
    check('its account is on the second line, after its worktree', under.every(function (r) {
      var a = account(r);
      var lane = r.querySelector('.ca-agentrow-chips');
      return a && lane && a.parentElement === lane && lane.firstElementChild.classList.contains('ca-wtchip') && lane.lastElementChild === a;
    }));
    if (grouped) {
      var headed = tops.filter(function (r) { return under.indexOf(r) < 0; });
      check('under a worktree’s heading, its account ends the second line, with no worktree chip', headed.length > 0 && headed.every(function (r) {
        var a = account(r);
        var lane = r.querySelector('.ca-agentrow-chips');
        return a && lane && lane.lastElementChild === a && !r.querySelector('.ca-wtchip');
      }), headed.length + ' rows under headings');
    }
  };

  var rowsOf = function (name) {
    return function () { return all(LISTS[name].rows).filter(shown); };
  };
  var rows = function (name) {
    var list = rowsOf(name);
    oneLine(name, list());
    report('one line');
    return Promise.resolve(rightClicks())
      .then(function () {
        rowsButton('Two lines').click();
        return until(function () { return list().length > 0 && list().every(function (r) { return r.classList.contains('is-nested') || !r.classList.contains('is-slim'); }); }, 3000, 'two lines');
      })
      .then(function () { return wait(200); })
      .then(function () {
        twoLines(name, list());
        report('two lines');
        rowsButton('One line').click();
        return until(function () { return localStorage.getItem(LISTS[name].key) === 'one'; }, 3000);
      });
  };

  say('chip lanes check: waiting for the list');
  until(function () { return document.querySelector('[role="option"]'); }, 20000)
    .then(function () { return wait(800); })
    .then(function () {
      var label = document.querySelector('[role="listbox"]').getAttribute('aria-label');
      check('Rows shows in Sessions, PRs and Todo', !!rowsButton('One line') === !!LISTS[label], label);
      if (LISTS[label]) return rows(label);
      report(label);
      return rightClicks();
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
