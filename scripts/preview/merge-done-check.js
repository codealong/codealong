// Checks Merge and done… (ui/MergeDone.tsx, menus.ts `mergeDone`): a session that's the
// primary of a PR ready to merge has it in its menu after Mark as done, with its key (⇧⌘M), and
// where the session has the strip over it (Sessions), first on the strip, before Seen, saying the
// key, the strip no taller for it; never in its header. It opens a dialog that says what it does, with the commit GitHub
// would write for the way kept for the repo (squash, GitHub's pick here) and the keyboard on its
// button. Cancel merges nothing. ⇧⌘M opens it too, in both windows. While GitHub is still working
// out whether it can merge, the dialog says so and reads it again, waiting for a read slower than
// its interval rather than asking over it, its button taking the keyboard once it can. A merge
// GitHub refuses says why in the dialog and leaves the session alone. Merged, the dialog goes,
// and the request that merged it had the engine mark the session done, work under way ended
// too, rather than the window. Once the PR isn't ready (merged, say), neither the strip's button
// nor the item is there, the strip's other buttons where they were, and ⇧⌘M opens nothing. A real
// session is made the primary of a real PR, made ready; the peek's card holds the dialog
// (SessionCard.tsx), the main window's Sessions App.tsx. In Sessions ⇧⌘M works from the session's
// box too, as ⇧⌘E does. Its keys are the platform's (PREVIEW_PLATFORM=linux: Ctrl+Shift+M).
// On the strip it says its words, then its key, wherever they fit, from a strip 420 to 1100px
// wide, the buttons on one line and inside it; in the notch, whose buttons never wrap, only that
// is checked.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/merge-done-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/merge-done-check.js
//   PREVIEW_WAIT=60000 PREVIEW_SIZE=1512,720 scripts/preview.sh notch '' scripts/preview/merge-done-check.js
(function () {
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  localStorage.removeItem('codealong.age');
  localStorage.removeItem('codealong.merge.methods');
  localStorage.removeItem('codealong.merge.deleteBranch');
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : null;
  };

  // A session on this Mac that runs in a terminal, and a PR to make it the primary of.
  var board = get('./board.json');
  // In the notch, one the floating bar's feed has, for its pill.
  var notched = !!(window.__BAR_MODE__ && window.__BAR_MODE__.mode === 'notch');
  var feed = notched ? get('./bar-feed.json') : null;
  var fed = {};
  (feed ? feed.items : []).forEach(function (i) { fed[i.key] = i; });
  var picked = null;
  var home = null;
  (board ? board.lanes : []).forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || l.device || s.archived || s.visitor || s.helper || (notched && !fed[s.key])) return;
      picked = s;
      home = l;
    });
  });
  var prs = (board && board.prs) || [];
  var pr = prs.filter(function (p) { return p.state !== 'merged' && p.state !== 'closed'; })[0] || prs[0];
  if (!picked || !pr) {
    say('merge done check: ' + (picked ? 'no PR' : 'no session') + ' on this Mac');
    stopped(picked ? 'no PR on the board to make ready' : 'no session on this Mac');
    return;
  }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  // Ready until it's merged, which a check below says it is.
  var merged = false;
  // GitHub still working out whether it can merge, the next time the box is read; and slow to
  // answer the time after that, longer than the dialog waits between reads (MergeBox.tsx
  // RECHECK_MS).
  var unsure = false;
  var slow = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'merge_box' && unsure) {
      unsure = false;
      slow = true;
      return real(cmd, args, options).then(function (b) { return Object.assign(b, { mergeable: 'UNKNOWN', mergeState: 'UNKNOWN' }); });
    }
    if (cmd === 'merge_box' && slow) {
      slow = false;
      return real(cmd, args, options).then(function (b) { return wait(4500).then(function () { return b; }); });
    }
    var got = real(cmd, args, options);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (s.key !== key) return;
          // The one that waits on you, at work on a command it left running.
          Object.assign(s, { status: 'unread', statusSince: Date.now() - 60e3, lastActivity: Date.now() - 60e3, asks: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null, background: 'gh pr checks --watch' });
        });
      });
      (b.prs || []).forEach(function (p) {
        if (p.primary === key) p.primary = undefined;
        if (p.key !== pr.key) return;
        Object.assign(p, {
          state: merged ? 'merged' : 'approved',
          merge: merged ? 'unknown' : 'clean',
          mergedAt: merged ? Date.now() : null,
          closedAt: null,
          checks: { passed: 3, failed: 0, pending: 0, skipped: 0, failing: [] },
          primary: key,
          sessions: [{ key: key, lane: home.id }].concat((p.sessions || []).filter(function (x) { return x.key !== key; })),
        });
      });
      return b;
    });
  };

  var peek = function () { return !!q('.ca-peek'); };
  var header = function () { return q('.ca-sessinsp-head'); };
  var strip = function () { return q('.ca-hotkeys'); };
  var stripButton = function (label) { return all('.ca-hotkeys .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0]; };
  var mergeButton = function () { return stripButton('Merge and done'); };
  var inHeader = function () { return all('.ca-sessinsp-head button').some(function (b) { return /Merge and done/.test(b.textContent); }); };
  // The session's menu once it has, or hasn't, its item: right-clicked again until it does.
  var menuWhen = async function (has, ms, what) {
    var until = Date.now() + ms;
    for (;;) {
      var labels = await menuOf(header(), { clientX: 40, clientY: 40 });
      if ((labels.indexOf(item) >= 0) === has) return labels;
      if (Date.now() > until) throw new Error(what);
      await wait(250);
    }
  };
  // ⇧⌘M (Ctrl+Shift+M on Linux), pressed where the keyboard is, or at the window.
  var mergeKey = function (at) { (at || window).dispatchEvent(new KeyboardEvent('keydown', { key: 'M', code: 'KeyM', metaKey: !linux, ctrlKey: linux, shiftKey: true, bubbles: true, cancelable: true })); };
  var dialog = function () { return q('.ca-mergedonedlg'); };
  var confirm = function () { return q('.ca-mergedonedlg .ca-btn--primary'); };
  var footButton = function (text) { return all('.ca-mergedonedlg .ca-dialog-foot button').filter(function (b) { return b.textContent === text; })[0]; };
  var titleField = function () { return q('.ca-mergedonedlg .ca-field input'); };
  var submit = function () { confirm().click(); };
  var item = 'Merge #' + pr.number + ' and done…';
  // Pushes a board to the page, as the engine does after a change.
  var push = function () {
    return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); });
  };

  // Its words, then its key, show wherever the strip has room for them beside the rest and the
  // least it keeps for what it says, and only there: at each width the buttons keep to one line
  // and inside the strip, and what's left out wouldn't have fit. Too narrow for the rest alone
  // (the notch's never wrap), they may wrap or run over with it saying no more than its icon.
  var sweep = async function (button) {
    var stripEl = button.closest('.ca-reply-queue.has-keys');
    var noteEl = q('.ca-reply-queue-note', stripEl);
    var shows = function (el) { return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'; };
    var wordsEl = q('span', button);
    var keyEl = q('kbd', button);
    var seenWords = false;
    var seenBare = false;
    var wrong = null;
    var widths = [420, 480, 540, 600, 660, 720, 780, 860, 960, 1100];
    for (var i = 0; i < widths.length && !wrong; i++) {
      stripEl.style.width = widths[i] + 'px';
      await wait(120);
      var buttons = all('.ca-hotkeys .ca-hotkey', stripEl);
      var tops = buttons.map(function (b) { return Math.round(b.getBoundingClientRect().top); });
      var end = stripEl.getBoundingClientRect().right - parseFloat(getComputedStyle(stripEl).paddingRight);
      var over = Math.max.apply(null, buttons.map(function (b) { return b.getBoundingClientRect().right; })) - end;
      var spare = noteEl.getBoundingClientRect().width - parseFloat(getComputedStyle(noteEl).minWidth);
      var gap = parseFloat(getComputedStyle(button).columnGap) || 0;
      var said = (shows(wordsEl) ? 'words ' : '') + (shows(keyEl) ? 'key' : '') || 'icon';
      wrong = tops.some(function (t) { return t !== tops[0]; }) && said !== 'icon'
        ? 'the buttons took two lines'
        : over > 0.5 && said !== 'icon'
          ? 'the buttons ran ' + over.toFixed(1) + 'px past the strip'
          : spare < -0.5
            ? 'what it says has ' + spare.toFixed(1) + 'px less than it keeps'
            : !shows(wordsEl) && shows(keyEl) && spare >= wordsEl.getBoundingClientRect().width + gap
              ? 'its words would have fit (' + spare.toFixed(1) + 'px spare)'
              : !shows(keyEl) && spare >= keyEl.getBoundingClientRect().width + gap
                ? 'its key would have fit (' + spare.toFixed(1) + 'px spare)'
                : shows(wordsEl) && !shows(keyEl)
                  ? 'its words without its key'
                  : null;
      if (shows(wordsEl)) seenWords = true;
      else seenBare = true;
      if (wrong) check('at ' + widths[i] + 'px the strip says as much of it as fits (' + said + ')', false, wrong);
    }
    if (!wrong) check('from ' + widths[0] + ' to ' + widths[widths.length - 1] + 'px the strip says as much of it as fits, on one line', seenWords && seenBare, seenWords ? 'its words never left' : 'its words never showed');
    stripEl.style.width = '';
    await wait(120);
  };

  // In the notch, whose buttons never wrap, only the strip is checked: the rest is the same
  // card's as the peek's. Its one pill is the session's, so the notch is as narrow as it gets.
  if (notched) {
    (async function () {
      try {
        say('merge done check: waiting for the notch');
        var shape = await until(function () { return q('.ca-notch') && calls('notch_shape').length > 0 && q('.ca-notch-shape'); }, 15000, 'the notch');
        var item = Object.assign({}, fed[key], { group: 'unread', status: 'unread', asks: false, snoozedUntil: null, hidden: false });
        window.__emit('bar-feed', Object.assign({}, feed, { loading: false, items: [item], recent: [], snoozed: [], reviews: [], todos: [] }));
        shape.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
        var pill = await until(function () { return q('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); }, 10000, 'no pill for “' + picked.title + '” in the notch');
        // Once it's open, as the pointer would find it.
        await wait(500);
        pill.click();
        await until(function () { return q('.ca-notch.is-session.is-open .ca-notch-session'); }, 5000, 'a click on its pill didn’t open the notch on “' + picked.title + '”');
        var button = await until(function () { return all('.ca-notch-session .ca-hotkeys .ca-hotkey').filter(function (b) { return b.textContent.indexOf('Merge and done') === 0; })[0]; }, 10000, 'no Merge and done on the notch’s strip');
        await sweep(button);
      } catch (e) {
        return stopped(e);
      }
      done();
    })();
    return;
  }

  (async function () {
    try {
      say('merge done check: waiting for the session');
      var rowOf = function () { return document.getElementById('row-' + home.id + '::' + key); };
      await until(function () { return peek() || rowOf(); }, 20000, 'neither the peek nor a row in Sessions for “' + picked.title + '”');
      if (!peek()) {
        // In Sessions, its row picked.
        var row = rowOf();
        row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
        row.click();
        await until(function () { var t = q('.ca-sessinsp-title'); return t && t.textContent === picked.title; }, 5000, 'Sessions didn’t show “' + picked.title + '”');
      }
      var stripped = !!strip();
      var labels = await menuWhen(true, 20000, 'no ' + item + ' in the menu of “' + picked.title + '”');
      var at = labels.indexOf(item);
      check('the session’s menu has it, after Mark as done', at > 0 && labels[at - 1] === 'Mark as done', labels.join(' | '));
      var accel = (lastMenu().filter(function (i) { return i.text === item; })[0] || {}).accelerator;
      check('with its key', accel === (linux ? 'Ctrl+Shift+M' : 'Shift+Cmd+M'), String(accel));
      check('never in the session’s header', !inHeader());
      var doneAt = null;
      var stripHeight = null;
      if (stripped) {
        var button = await until(mergeButton, 5000, 'no Merge and done on the strip over “' + picked.title + '”');
        check('the strip has it first, before Seen, saying its key', all('.ca-hotkeys .ca-hotkey')[0] === button && stripButton('Seen') && !!(button.compareDocumentPosition(stripButton('Seen')) & Node.DOCUMENT_POSITION_FOLLOWING) && (q('kbd', button) || {}).textContent === (linux ? 'Ctrl+Shift+M' : '⇧⌘M'), button.textContent);
        await sweep(button);
        doneAt = stripButton('Done').getBoundingClientRect().left;
        stripHeight = q('.ca-hotkeys').getBoundingClientRect().height;
      } else {
        note('no strip over the session here: the strip’s button not tried');
      }

      // Merged elsewhere: nothing to merge, until it's ready again.
      merged = true;
      await push();
      labels = await menuWhen(false, 5000, 'its item stayed once the PR merged');
      check('Merge and done… goes from the menu once the PR has merged', labels.indexOf(item) < 0, labels.join(' | '));
      if (stripped) {
        var gone = await until(function () { return !mergeButton(); }, 5000).then(function () { return true; }, function () { return false; });
        check('and from the strip, its other buttons where they were', gone && stripButton('Done').getBoundingClientRect().left === doneAt, stripButton('Done').getBoundingClientRect().left + ' vs ' + doneAt);
        check('it took no line of the strip’s own', q('.ca-hotkeys').getBoundingClientRect().height === stripHeight, stripHeight + ' with it, ' + q('.ca-hotkeys').getBoundingClientRect().height + ' without');
      }
      mergeKey();
      await wait(300);
      check('⇧⌘M opens nothing then', !dialog());
      merged = false;
      await push();
      await menuWhen(true, 5000, 'its item didn’t come back with the PR ready again');
      if (stripped) await until(mergeButton, 5000, 'Merge and done didn’t come back to the strip with the PR ready again');
      check('ready again, it comes back', true);

      // From the menu: the dialog, and Cancel.
      menuItem(item)();
      var dlg = await until(dialog, 5000, 'the menu’s item opened no dialog');
      var lede = (q('.ca-dialog-lede', dlg) || {}).textContent || '';
      check('the dialog says it merges, then marks it done', lede.indexOf('Merges #' + pr.number) === 0 && lede.indexOf('then marks “' + picked.title + '” done') > 0, lede);
      check('and that the command it left running stops', /gh pr checks --watch`, which it left running in the background, stops with it\./.test(lede), lede);
      var go = await until(function () { var b = confirm(); return b && !b.disabled ? b : null; }, 5000, 'its button never read the box');
      check('its button has the keyboard', document.activeElement === go, document.activeElement ? document.activeElement.outerHTML.slice(0, 80) : 'nothing');
      var method = q('.ca-mergedonedlg-method .ca-modelmenu-chip');
      check('it merges the way GitHub picks for you, kept for the repo', !!method && method.textContent === 'Squash and merge', method ? method.textContent : 'no method');
      check('with the commit GitHub would write for it', !!titleField() && titleField().value === pr.title + ' (#' + pr.number + ')', titleField() ? titleField().value : 'no title');
      footButton('Cancel').click();
      await until(function () { return !dialog(); }, 3000, 'Cancel left the dialog up');
      check('Cancel merges nothing', calls('merge_pr').length === 0 && calls('session_done').length === 0, calls('merge_pr').length + ' merges');
      if (stripped) {
        // ⇧⌘M, where the strip says it, from the session's box as from anywhere.
        var box = q('.ca-sessinsp .ca-composer textarea');
        if (box) box.focus();
        mergeKey(box || undefined);
        await until(dialog, 3000, '⇧⌘M opened no dialog' + (box ? ' from the session’s box' : ''));
        check('⇧⌘M opens it too, from the session’s box', !!box, box ? 'from the box' : 'no box to press it in');
        footButton('Cancel').click();
        await until(function () { return !dialog(); }, 3000, 'Cancel left the dialog up');
      }

      // From ⇧⌘M, or the strip where there's one, GitHub still working it out: read again until
      // it can merge.
      unsure = true;
      var readsBefore = calls('merge_box').length;
      if (stripped) mergeButton().click();
      else mergeKey();
      var still = await until(function () { return q('.ca-mergedonedlg .ca-merge-standing strong'); }, 5000, 'the dialog said nothing while GitHub worked it out');
      check('while GitHub works out whether it can merge, the dialog says so and can’t merge yet', still.textContent === 'Checking whether it can merge…' && confirm().disabled, still.textContent);
      go = await until(function () { var b = confirm(); return b && !b.disabled ? b : null; }, 15000, 'the dialog never read the box again');
      check('it waited for the slow read rather than asking again over it', calls('merge_box').length - readsBefore === 2, (calls('merge_box').length - readsBefore) + ' reads');
      check('read again, it can, and its button has the keyboard', document.activeElement === go, document.activeElement ? document.activeElement.outerHTML.slice(0, 80) : 'nothing');

      // Refused, then merged.
      type(titleField(), 'fail');
      submit();
      var error = await until(function () { return q('.ca-mergedonedlg .ca-form-error'); }, 5000, 'a refused merge said nothing');
      check('a merge GitHub refuses says why in the dialog', /Head branch was modified/.test(error.textContent), error.textContent);
      check('and leaves the session as it was', calls('session_done').length === 0 && !!dialog(), calls('session_done').length + ' done');
      type(titleField(), 'Ship it (#' + pr.number + ')');
      submit();
      await until(function () { return !dialog(); }, 5000, 'the dialog stayed up');
      var op = sent('merge_pr').pop();
      check('it merges the PR with what was written', !!op && op.repo === pr.repoSlug && op.number === pr.number && op.op.op === 'merge' && op.op.method === 'squash' && op.op.title === 'Ship it (#' + pr.number + ')', JSON.stringify(op));
      check('and the same request marks the session done, not the window', op.done === key && calls('session_done').length === 0, op.done + ', ' + calls('session_done').length + ' asked by the window');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
