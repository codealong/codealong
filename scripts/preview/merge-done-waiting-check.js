// Checks Merge and done… (ui/MergeDone.tsx, menus.ts) on the last session that waits in the
// waiting window: confirmed, the dialog goes and the window goes on at once, to the end of what
// waits, before GitHub answers (the stub takes a moment). A merge GitHub refuses (the stub refuses
// a commit titled "fail") brings the window back to the session, the strip saying why, and the
// dialog back on it with the commit as it was written, its button not taking the keyboard. Merged
// then, the window goes on and the dialog stays gone. One session waits on you, the primary of a
// PR ready to merge; no PR asks for your review.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/merge-done-waiting-check.js
(function () {
  localStorage.removeItem('codealong.merge.methods');
  localStorage.removeItem('codealong.merge.deleteBranch');
  var now = Date.now();
  var picked = null;
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var lane = null;
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || s.helper || (picked && picked !== s.key)) return;
          picked = s.key;
          lane = l;
          Object.assign(s, { status: 'unread', statusSince: now - 600e3, lastActivity: now - 600e3, asks: false, title: 'Queue A', background: null });
        });
      });
      var open = (board.prs || []).filter(function (x) { return x.state !== 'merged' && x.state !== 'closed'; });
      var ready = (pr && open.filter(function (x) { return x.key === pr.key; })[0]) || open[0];
      board.prs = (board.prs || []).map(function (x) {
        var y = Object.assign({}, x, { reviewRequested: false });
        if (y.primary === picked) y.primary = undefined;
        if (!ready || y.key !== ready.key || !picked) return y;
        Object.assign(y, {
          state: 'approved', merge: 'clean', mergedAt: null, closedAt: null, primary: picked,
          checks: { passed: 3, failed: 0, pending: 0, skipped: 0, failing: [] },
          sessions: [{ key: picked, lane: lane ? lane.id : '' }].concat((y.sessions || []).filter(function (z) { return z.key !== picked; })),
        });
        if (!pr) pr = y;
        return y;
      });
      return board;
    });
  };

  var sessionTitle = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = q('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  var atEnd = function () { return !!q('.ca-queue-end'); };
  var dialog = function () { return q('.ca-mergedonedlg'); };
  var confirmButton = function () { return q('.ca-mergedonedlg .ca-btn--primary'); };
  var titleField = function () { return q('.ca-mergedonedlg .ca-field input'); };
  var messageField = function () { return q('.ca-mergedonedlg textarea'); };
  var mergeButton = function () { return all('.ca-hotkeys .ca-hotkey').filter(function (b) { return b.textContent.indexOf('Merge and done') === 0; })[0]; };
  /** Opens the dialog from the strip, once its box is read and it can merge. */
  var ask = async function () {
    (await until(mergeButton, 10000, 'no Merge and done on the strip over “Queue A”')).click();
    await until(function () { var b = confirmButton(); return b && !b.disabled && titleField(); }, 5000, 'the dialog never read the box');
  };

  (async function () {
    try {
      say('merge done waiting check: waiting for the window');
      await until(function () { return !!pr && sessionTitle() === 'Queue A'; }, 20000, 'the window on “Queue A”, the primary of a ready PR');
      await ask();
      type(titleField(), 'fail');
      type(messageField(), 'Written here');
      confirmButton().click();
      await wait(50);
      check('confirmed, the dialog goes and the window goes on at once, before GitHub answers', !dialog() && atEnd() && sent('merge_pr').length === 1, (dialog() ? 'dialog up, ' : '') + (atEnd() ? 'at the end' : 'on ' + sessionTitle()));

      var back = await until(function () { return sessionTitle() === 'Queue A' && titleField() && q('.ca-mergedonedlg .ca-form-error'); }, 5000, 'a refused merge didn’t bring the window and the dialog back to “Queue A”');
      check('a refused merge brings the window back to the session', true);
      check('the strip saying why', /Head branch was modified/.test(strip()), strip());
      check('and the dialog back on it, saying why', /Head branch was modified/.test(back.textContent), back.textContent);
      check('with the commit as it was written', titleField().value === 'fail' && messageField().value === 'Written here', titleField().value + ' / ' + messageField().value);
      check('its button not taking the keyboard', document.activeElement !== confirmButton(), document.activeElement ? document.activeElement.outerHTML.slice(0, 80) : 'nothing');

      type(titleField(), 'Ship it (#' + pr.number + ')');
      confirmButton().click();
      await until(function () { return !dialog() && atEnd(); }, 3000, 'the window didn’t go on once confirmed again');
      await wait(700);
      check('merged, the window stays on, the dialog gone', !dialog() && atEnd() && sent('merge_pr').length === 2 && sent('merge_pr')[1].op.title === 'Ship it (#' + pr.number + ')', JSON.stringify(sent('merge_pr')[1]));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
