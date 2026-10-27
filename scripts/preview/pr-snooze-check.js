// Checks snoozing a review in the PRs view (PrsView.tsx, PrInspector.tsx, snooze.ts): rows have
// no Approve button (it's in the PR's panel and menu); a review asked of you has Snooze review in
// its menu, which snoozes it and moves its row to Snoozed at once (ahead.ts); a snoozed review
// waits under Snoozed with a chip that says when it's back; and its panel says until when, keeps
// Approve first, and its Snooze… wakes it. Each project's two latest open PRs are made reviews
// asked of you, the second snoozed for three hours, since a snapshot seldom has a snoozed one;
// others may wait on your review already.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-snooze-check.js
(function () {
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var snoozes = function () { return calls('snooze_reviews').map(function (c) { return c[1]; }); };
  /** A section of the view by its heading, and its rows. */
  var section = function (label) {
    return all('.ca-bayblock').filter(function (b) {
      var head = q('.ca-bay > span', b);
      return head && head.textContent === label;
    })[0];
  };
  var rowsOf = function (label) {
    var s = section(label);
    return s ? all('.ca-prrow', s) : [];
  };
  var number = function (row) { return (q('.ca-prrow-num', row) || {}).textContent; };
  var numbers = function (label) { return rowsOf(label).map(number); };
  /** A row of a section by its PR's number (`#12`). */
  var rowIn = function (label, n) { return rowsOf(label).filter(function (r) { return number(r) === n; })[0]; };
  /** The sidebar's count of reviews that wait on you, 0 with no badge. */
  var reviewCount = function () {
    var b = all('.ca-nav-badge').filter(function (x) { return /want your review/.test(x.title); })[0];
    return b ? Number(b.textContent) : 0;
  };

  var now = Date.now();
  /** The review the check snoozes and wakes, as `#12`. */
  var picked = null;
  var asked = [];
  var snoozed = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; });
      board.projects.forEach(function (project) {
        var mine = open.filter(function (pr) { return pr.project === project.name; });
        if (mine[0]) {
          mine[0].reviewRequested = true;
          mine[0].mine = false;
          mine[0].snoozedUntil = null;
          asked.push('#' + mine[0].number);
        }
        if (mine[1]) {
          mine[1].reviewRequested = true;
          mine[1].mine = false;
          mine[1].snoozedUntil = now + 3 * 3600e3;
          snoozed.push('#' + mine[1].number);
        }
      });
      board.pinnedPrs = [];
      return board;
    });
  };

  say('pr snooze check: waiting for PR rows');
  until(function () { return q('.ca-prrow'); }, 20000)
    .then(function () {
      check('no PR row has a button of its own', all('.ca-prrow button.ca-btn').length === 0, all('.ca-prrow button.ca-btn').map(function (b) { return b.textContent; }).join(', '));
      var n = asked.filter(function (x) { return rowIn('Review requested', x); })[0];
      if (!n) throw new Error('no review the check asked of you in the project shown: it has no open PR');
      picked = n;
      var row = rowIn('Review requested', n);
      var later = snoozed.filter(function (x) { return rowIn('Snoozed', x); })[0];
      check('a review asked of you is under Review requested', !!row, numbers('Review requested').join());
      check('with its amber edge', row.classList.contains('is-flag-attn'), row.className);
      if (!later) note('the project shown has one open PR, so nothing starts snoozed');
      else {
        var asleep = rowIn('Snoozed', later);
        check('a snoozed one waits under Snoozed, not Review requested', !rowIn('Review requested', later), numbers('Review requested').join());
        var chip = q('.ca-snoozechip', asleep);
        check('with a chip that says when it is back', chip && chip.textContent === '3h' && /^Snoozed until .+: out of Review requested and off the floating bar until then, then back$/.test(chip.title), chip && chip.textContent + ' / ' + chip.title);
        check('and no amber edge', !asleep.classList.contains('is-flag-attn'), asleep.className);
      }
      var count = reviewCount();
      check('the sidebar counts the reviews under Review requested', count === rowsOf('Review requested').length, count + ' for ' + numbers('Review requested').join());
      return menuOf(row).then(function (labels) {
        var approve = labels.indexOf('Approve ' + n);
        check('its menu has Approve and Snooze review together', approve >= 0 && labels[approve + 1] === 'Snooze review › For 15 minutes', labels.slice(Math.max(approve, 0)).join(' | '));
        var at = Date.now();
        menuItem('Snooze review › For 1 hour')();
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('Snooze review snoozes it for an hour', s && s.keys.length === 1 && s.keys[0].slice(-n.length) === n && Math.abs(s.until - at - 3600e3) < 60e3, JSON.stringify(s));
          check('and moves its row to Snoozed at once', !rowIn('Review requested', n) && !!rowIn('Snoozed', n), numbers('Snoozed').join());
          check('the soonest back first', numbers('Snoozed')[0] === n, numbers('Snoozed').join());
          check('and the sidebar stops counting it', reviewCount() === count - 1, String(reviewCount()));
          return rowIn('Snoozed', n);
        });
      });
    })
    .then(function (row) {
      press(row);
      return until(function () { return q('.ca-prinsp'); }, 5000, 'no PR panel').then(function () {
        var panel = q('.ca-prinsp');
        var tag = all('.ca-prinsp-status .ca-tag', panel).map(function (t) { return t.textContent; });
        check('its panel says until when its review is snoozed', tag.some(function (t) { return /^Review snoozed until /.test(t); }) && tag.indexOf('Your review requested') < 0, tag.join(' | '));
        var buttons = all('.ca-insp-actions button', panel).map(function (b) { return b.textContent; });
        check('and keeps Approve first, and Snooze…', buttons[0] === 'Approve' && buttons.indexOf('Snooze…') > 0, buttons.join(' | '));
        var snooze = all('.ca-insp-actions button', panel).filter(function (b) { return b.textContent === 'Snooze…'; })[0];
        var made = menus();
        snooze.click();
        return until(function () { return menus() > made; }, 3000, 'no menu from Snooze…').then(function () {
          var labels = menuLabels();
          check('Snooze… drops the times, Pick a time… and Wake now', labels.join('|').indexOf('For 15 minutes|For 30 minutes|For 1 hour|For 3 hours|Until ') === 0 && labels.slice(-2).join('|') === 'Pick a time…|Wake now', labels.join(' | '));
          menuItem('Wake now')();
          return wait(100);
        });
      });
    })
    .then(function () {
      var s = snoozes().pop();
      check('Wake now wakes it', s && s.keys.length === 1 && s.until === null, JSON.stringify(s));
      check('and it waits on you again at once', !!rowIn('Review requested', picked) && !rowIn('Snoozed', picked), numbers('Review requested').join() + ' / snoozed ' + numbers('Snoozed').join());
    })
    .then(done)
    .catch(stopped);
})();
