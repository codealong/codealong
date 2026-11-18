// Checks the strip over a PR's panel in the PRs view (PrStrip.tsx, PrInspector.tsx): it has what a
// PR's card has on its strip (Approve, Comment, Snooze, Review with and GitHub, each saying its key)
// and no Next, which is the list's; Approve is there for an open PR that isn't yours though your
// review wasn't asked for, and approves it; Snooze is dimmed with no review asked; the merge box's
// button leads the row under the PR's title (MergeBox.tsx), the strip having the rest; the PR's
// menu has Approve too. An approval GitHub is slow to answer is still on its way after another PR
// was shown and this one again: Approve is dimmed, and ⌘⏎ in the box sends no second one; refused
// while another PR shows, the window says why. Your own PR's Approve is dimmed and says why, and
// its menu has none (GitHub doesn't let you approve your own). In each project shown, the latest
// open PR is made someone else's that doesn't ask for your review, and the next your own.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-approve-check.js
(function () {
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var others = [];
  var yours = [];
  // While `slow`, GitHub takes two seconds over a review and refuses it.
  var slow = false;
  var asked = 0;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'review_pr' && slow) {
      asked += 1;
      return new Promise(function (_, reject) { setTimeout(function () { reject('GitHub took too long'); }, 2000); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; });
      board.projects.forEach(function (project) {
        var here = open.filter(function (pr) { return pr.project === project.name; }).sort(function (a, b) { return b.updatedAt - a.updatedAt; });
        if (here[0]) {
          Object.assign(here[0], { state: 'open', mine: false, author: 'ana', reviewRequested: false, snoozedUntil: null });
          others.push('#' + here[0].number);
        }
        if (here[1]) {
          Object.assign(here[1], { state: 'open', mine: true, reviewRequested: false, snoozedUntil: null });
          yours.push('#' + here[1].number);
        }
      });
      board.pinnedPrs = [];
      return board;
    });
  };
  var number = function (row) { return (q('.ca-prrow-num', row) || {}).textContent; };
  var row = function (n) { return all('.ca-prrow').filter(function (r) { return number(r) === n; })[0]; };
  var strip = function () { return q('.ca-prpanel > .ca-reply-queue'); };
  var label = function (b) { var s = q(':scope > span', b); return s ? s.textContent : ''; };
  // Every button the strip has, those its ⋯ holds for want of room too.
  var hotkeys = function () { return strip() ? all('.ca-hotkey', strip()).filter(label) : []; };
  var hotkey = function (name) { return hotkeys().filter(function (b) { return label(b) === name; })[0]; };
  var shown = function () { return q('.ca-prinsp .ca-insp-call') && q('.ca-prinsp .ca-insp-call').textContent; };
  var buttons = function () { return all('.ca-prinsp .ca-insp-actions > .ca-btn, .ca-prinsp .ca-insp-actions > .ca-split > .ca-btn:first-child'); };
  var merge = function () { return q('.ca-prinsp .ca-insp-actions .ca-merge-main'); };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var theirs, mineN;

  say('pr approve check: waiting for PR rows');
  until(function () { return q('.ca-prrow'); }, 20000)
    .then(function () {
      theirs = others.filter(row)[0];
      mineN = yours.filter(row)[0];
      if (!theirs) throw new Error('the project shown has no open PR');
      press(row(theirs));
      return until(function () { return shown() === theirs && !!merge() && !!hotkey('Approve'); }, 5000, 'the strip over its panel');
    })
    .then(function () {
      var names = hotkeys().map(label);
      check('the strip has the PR’s actions, as its card’s has', names.join('|') === 'Approve|Comment|Snooze|Review with|GitHub', names.join(' | '));
      check('each saying its key', hotkeys().every(function (b) { return !!q('kbd', b); }), hotkeys().map(function (b) { return b.textContent; }).join(' | '));
      check('and no Next: the list goes through what waits on you', !all('.ca-hotkey', strip()).some(function (b) { return label(b) === 'Next'; }), strip().textContent);
      check('an open PR that isn’t yours has Approve, though your review wasn’t asked for', !hotkey('Approve').disabled, hotkey('Approve').title);
      check('Approve says ⌘⏎ is the box’s', /from the box\)$/.test(hotkey('Approve').title), hotkey('Approve').title);
      check('Snooze is dimmed with no review asked', hotkey('Snooze').disabled && /isn’t asked for/.test(hotkey('Snooze').title), hotkey('Snooze').title);
      check('the merge button leads the row under the title, with nothing of the panel’s beside it', buttons()[0] === q('.ca-merge-main > .ca-btn') && !buttons().some(function (b) { return /^(Approve|GitHub|Review with…|Snooze…)$/.test(b.textContent); }), buttons().map(function (b) { return b.textContent; }).join(' | '));
      hotkey('Approve').click();
      return until(function () { return sent('review_pr').length === 1; }, 3000, 'the approval');
    })
    .then(function () {
      var r = sent('review_pr')[0];
      check('Approve approves it', r.verdict === 'approve' && '#' + r.number === theirs && r.body === '', JSON.stringify(r));
      return menuOf(row(theirs));
    })
    .then(function (items) {
      check('its menu has Approve', items.indexOf('Approve ' + theirs) >= 0, items.join(' | '));
      // Another PR to show meanwhile: yours, else any other listed.
      var away = mineN || all('.ca-prrow').map(number).filter(function (n) { return n !== theirs; })[0];
      if (!away) return note('one PR listed: no other to show while an approval is on its way');
      slow = true;
      return until(function () { return !hotkey('Approve').disabled; }, 3000, 'Approve after the first approval')
        .then(function () {
          hotkey('Approve').click();
          press(row(away));
          return until(function () { return shown() === away; }, 3000, 'another PR’s panel');
        })
        .then(function () {
          press(row(theirs));
          return until(function () { return shown() === theirs && !!hotkey('Approve'); }, 3000, 'the PR again');
        })
        .then(function () {
          check('back on it before GitHub answers, Approve is dimmed: one is on its way', hotkey('Approve').disabled && /^Sending/.test(hotkey('Approve').title), hotkey('Approve').title);
          var box = q('.ca-prpanel .ca-prreview-box textarea');
          box.focus();
          box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
          check('and ⌘⏎ in its box sends no second one', asked === 1, String(asked));
          press(row(away));
          return until(function () { return /^Didn’t approve /.test(toast()); }, 4000, 'the refusal said while another PR shows');
        })
        .then(function () {
          check('refused while another PR shows, the window says why', /GitHub took too long/.test(toast()), toast());
          slow = false;
        });
    })
    .then(function () {
      if (!mineN) {
        note('the project shown has one open PR, so none is yours');
        return null;
      }
      press(row(mineN));
      return until(function () { return shown() === mineN && !!merge() && !!hotkey('Approve'); }, 5000, 'your PR’s panel').then(function () {
        check('your own PR’s Approve is dimmed, and says why', hotkey('Approve').disabled && /approve your own/.test(hotkey('Approve').title), hotkey('Approve').title);
        check('its merge button leads the row', buttons()[0] === q('.ca-merge-main > .ca-btn'), buttons().map(function (b) { return b.textContent; }).join(' | '));
        return menuOf(row(mineN));
      });
    })
    .then(function (items) {
      if (items) check('nor in its menu', !items.some(function (x) { return /^Approve /.test(x); }), items.join(' | '));
    })
    .then(done, stopped);
})();
