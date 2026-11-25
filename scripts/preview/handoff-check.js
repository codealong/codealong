// Checks cross-provider session handoff via the all-models account selector + quick confirmation
// modal in the bar's peek (handoff.ts, MoveAccount.tsx, SessionAgent.tsx, menus.ts): the session's
// right-click Move to submenu lists cross-provider accounts, the reply-box account selector lists
// all models for every eligible account, and picking a model under another provider opens the
// quick confirmation modal explaining how moving across accounts works, with Keep original open
// and passing the chosen model to move_session.
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

      // 2. Account selector shows all models for cross-provider accounts too.
      var chip = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); }, 5000, 'no account chip');
      chip.click();
      var menu = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu .ca-picker'); }, 3000, 'no account menu');
      var crossSec = all('.ca-picker-section', menu).filter(function (sec) {
        var h = q('.ca-quick-acct-name', sec);
        return h && h.textContent === crossTarget.name;
      })[0];
      var opts = crossSec ? all('.ca-picker-opt', crossSec) : [];
      check('the account selector lists the other provider and its models', !!crossSec && opts.length > 0, crossSec ? opts.length + ' options' : 'missing section');
      // Pick an explicit model (second option if available, else default).
      var pickedOpt = opts[1] || opts[0];
      var pickedModelTitle = q('.ca-picker-title', pickedOpt).textContent.trim();
      pickedOpt.click();

      // 3. Quick confirmation modal explains how moving across accounts works.
      var dlg = await until(dialog, 3000, 'picking a model under another account opened no confirmation modal');
      var title = q('.ca-dialog-head h2', dlg).textContent;
      var lede = q('.ca-dialog-lede', dlg).textContent;
      check('modal title names the target provider', /^Move to /.test(title), title);
      check('lede briefly explains handoff brief, full transcript, and archive fate', /handoff brief/.test(lede) && /full transcript/.test(lede) && /closed and archived/.test(lede), lede);
      if (opts.length > 1) check('lede names the picked model', lede.indexOf('on ' + pickedModelTitle) >= 0, lede + ' | ' + pickedModelTitle);

      // 4. Toggling Keep original open updates title, lede, and primary button.
      var keepInput = q('.ca-movedlg-keep input[type="checkbox"]', dlg);
      check('modal offers Keep original open toggle', !!keepInput && !keepInput.checked, 'missing or checked');
      keepInput.click();
      await until(function () { return q('.ca-dialog-head h2', dlg).textContent === 'Hand off to another account'; }, 2000, 'title did not update for Keep original open');
      lede = q('.ca-dialog-lede', dlg).textContent;
      var go = q('.ca-btn--primary', dlg);
      check('Keep original open updates lede and primary button', /this session stays open\./.test(lede) && go && go.textContent === 'Hand off to ' + crossTarget.name, lede + ' | ' + (go ? go.textContent : 'no button'));

      // 5. Confirming passes account, model, and keep: true without closing the peek.
      var hidesBefore = calls('peek_hide').length;
      go.click();
      await until(function () { return calls('move_session').length > 0; }, 3000, 'confirming asked for no move_session');
      var asked = calls('move_session')[0][1];
      check('move_session receives target account, keep: true, and chosen model', asked.key === key && asked.account === crossTarget.name && asked.keep === true && (opts.length <= 1 || !!asked.model), JSON.stringify(asked));
      await until(function () { return !dialog(); }, 3000, 'modal stayed up after confirm');
      await wait(150);
      check('keeping the session open leaves the peek open rather than hiding it', calls('peek_hide').length === hidesBefore, String(calls('peek_hide').length - hidesBefore));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
