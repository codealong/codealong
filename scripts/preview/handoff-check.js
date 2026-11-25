// Checks cross-provider session handoff in the bar's peek (handoff.ts, MoveAccount.tsx,
// SessionAgent.tsx, menus.ts): a Claude Code session offers cross-provider accounts in both its
// right-click Move to submenu and the reply box's account chip (`Hand off here`); picking one
// opens the unified Move / Hand off dialog with handoff options (Opens in, Model, Effort, Next
// step, Keep this session open), updates the lede and primary button live, and sends move_session
// with those options; non-Claude sessions (e.g. Codex) offer handoff to other providers too.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/handoff-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };

  var board = get('./board.json');
  var allAccounts = get('./accounts.json').filter(function (a) { return a.signedIn && !a.disabled; });
  var picked = null;
  var crossTarget = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || l.device || s.archived || s.visitor || s.harness !== 'claude' || !s.resumeCommand) return;
      var otherProvider = allAccounts.filter(function (a) { return a.harness !== s.harness && a.name !== s.account; })[0];
      if (otherProvider) {
        picked = s;
        crossTarget = otherProvider;
      }
    });
  });
  if (!picked || !crossTarget) { stopped('no Claude Code session with a signed-in account on another provider'); return; }

  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'move_session') {
      window.__calls.push([cmd, arguments[1]]);
      var args = arguments[1] || {};
      var verb = args.keep ? 'Handed off' : 'Moved';
      return Promise.resolve({
        key: crossTarget.harness + ':' + args.account + ':f0000000-0000-4000-8000-000000000001',
        message: verb + ' “' + picked.title + '” to ' + args.account + '.',
      });
    }
    return real.apply(this, arguments);
  };

  var dialog = function () { return q('.ca-movedlg'); };

  (async function () {
    try {
      // 1. Context menu lists cross-provider targets with provider name.
      var more = await until(function () { return q('.ca-sessinsp-head [aria-label="More"]'); }, 20000, 'no More button');
      var n = menus();
      more.click();
      await until(function () { return menus() > n; }, 3000, 'menu did not open');
      var labels = window.__menu().map(function (x) { return x[0]; });
      var hasCrossMenu = labels.some(function (l) { return l.indexOf('Move to › ' + crossTarget.name + ' · ') === 0 || l.indexOf('Move to ' + crossTarget.name + ' · ') === 0; });
      check('the session menu lists cross-provider accounts with their provider name', hasCrossMenu, labels.join(' | '));
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));

      // 2. Reply box account chip lists cross-provider targets with "Hand off here".
      var chip = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); }, 5000, 'no account chip');
      chip.click();
      var menu = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu .ca-picker'); }, 3000, 'no account menu');
      var handoffOpts = all('.ca-picker-opt', menu).filter(function (o) {
        return q('.ca-picker-title', o) && q('.ca-picker-title', o).textContent.trim() === 'Hand off here';
      });
      check('the account chip menu offers Hand off here for other providers', handoffOpts.length > 0, all('.ca-picker-title', menu).map(function (t) { return t.textContent; }).join(' | '));
      handoffOpts[0].click();

      // 3. MoveDialog opens in generic handoff mode with handoff brief lede and controls.
      var dlg = await until(dialog, 3000, 'picking Hand off here opened no dialog');
      var title = q('.ca-dialog-head h2', dlg).textContent;
      var lede = q('.ca-dialog-lede', dlg).textContent;
      check('cross-provider dialog names the target provider in its title', /^Move to /.test(title), title);
      check('its lede describes the handoff brief and full transcript link', /handoff brief/.test(lede) && /full transcript/.test(lede) && /closed and archived/.test(lede), lede);
      var noteBox = q('.ca-movedlg-note textarea', dlg);
      check('it shows an optional next-step note box', !!noteBox, 'no textarea');

      // 4. Typing a next-step note and toggling Keep this session open updates lede, title, and button.
      var setVal = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setVal.call(noteBox, 'Finish fixing the flaky test');
      noteBox.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(50);
      var keepInput = q('.ca-movedlg-keep input[type="checkbox"]', dlg);
      check('it shows a Keep this session open checkbox', !!keepInput && !keepInput.checked, 'missing or checked');
      keepInput.click();
      await until(function () { return q('.ca-dialog-head h2', dlg).textContent === 'Hand off to another account'; }, 2000, 'title did not switch to Hand off');
      lede = q('.ca-dialog-lede', dlg).textContent;
      var go = q('.ca-btn--primary', dlg);
      check('keeping open updates lede and primary button', /with your next-step note; this session stays open\./.test(lede) && /^Hand off to /.test(go.textContent), lede + ' | ' + go.textContent);

      // 5. Submitting passes keep: true and prompt to move_session without leaving/archiving.
      var hidesBefore = calls('peek_hide').length;
      go.click();
      await until(function () { return calls('move_session').length > 0; }, 3000, 'Hand off asked for no move_session');
      var asked = calls('move_session')[0][1];
      check('move_session receives keep: true and the next-step prompt', asked.key === key && asked.keep === true && asked.prompt === 'Finish fixing the flaky test', JSON.stringify(asked));
      await until(function () { return !dialog(); }, 3000, 'dialog stayed up after Hand off');
      await wait(150);
      check('keeping the session open leaves the peek open rather than hiding it', calls('peek_hide').length === hidesBefore, String(calls('peek_hide').length - hidesBefore));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
