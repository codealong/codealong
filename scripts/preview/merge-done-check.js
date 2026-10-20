// Checks Merge and done… (ui/MergeDone.tsx, menus.ts `mergeDone`): a session that's the
// primary of a PR ready to merge has it beside the PR's chip in its header, and in its menu after
// Mark as done. It opens a dialog that says what it does, with the commit GitHub would write for
// the way kept for the repo (squash, GitHub's pick here) and the keyboard on its button. Cancel
// merges nothing. While GitHub is still working out whether it can merge, the dialog says so and
// reads it again, waiting for a read slower than its interval rather than asking over it, its
// button taking the keyboard once it can. A merge GitHub refuses says why in
// the dialog and leaves the session alone. Merged, the dialog goes, and the request that merged
// it had the engine mark the session done, work under way ended too, rather than the window. Once
// the PR isn't ready (merged, say), neither the button nor the item is there. A real session is made
// the primary of a real PR, made ready; the peek's card holds the dialog (SessionCard.tsx), the
// main window's Sessions App.tsx.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/merge-done-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/merge-done-check.js
(function () {
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
  var picked = null;
  var home = null;
  (board ? board.lanes : []).forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || l.device || s.archived || s.visitor || s.helper) return;
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
  var mergeButton = function () { return q('.ca-sessinsp-links .ca-sessinsp-merge'); };
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
      var button = await until(mergeButton, 20000, 'no Merge and done… in the header of “' + picked.title + '”');
      var chip = q('.ca-sessinsp-links .ca-prchipcard, .ca-sessinsp-links .ca-prchip');
      check('Merge and done… follows the PR’s chip', !!chip && !!(chip.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING), chip ? 'chip before it' : 'no chip');
      check('it’s as tall as the chips', Math.round(button.getBoundingClientRect().height) <= Math.round((chip || button).getBoundingClientRect().height) + 1, button.getBoundingClientRect().height + ' vs ' + (chip ? chip.getBoundingClientRect().height : '?'));

      var labels = await menuOf(header(), { clientX: 40, clientY: 40 });
      var at = labels.indexOf(item);
      check('the session’s menu has it, after Mark as done', at > 0 && labels[at - 1] === 'Mark as done', labels.join(' | '));

      // Merged elsewhere: nothing to merge, until it's ready again.
      merged = true;
      await push();
      var gone = await until(function () { return !mergeButton(); }, 5000).then(function () { return true; }, function () { return false; });
      check('Merge and done… goes once the PR has merged', gone);
      labels = await menuOf(header(), { clientX: 40, clientY: 40 });
      check('and so does its item', labels.indexOf(item) < 0, labels.join(' | '));
      merged = false;
      await push();
      await until(mergeButton, 5000, 'Merge and done… didn’t come back with the PR ready again');
      labels = await menuOf(header(), { clientX: 40, clientY: 40 });
      check('ready again, both come back', labels.indexOf(item) > 0, labels.join(' | '));

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

      // From the header, GitHub still working it out: read again until it can merge.
      unsure = true;
      var readsBefore = calls('merge_box').length;
      mergeButton().click();
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
