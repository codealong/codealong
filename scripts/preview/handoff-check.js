// Checks inline cross-provider session handoff in the bar's peek (handoff.ts, HandoffStrip.tsx,
// SessionAgent.tsx, SessionReply.tsx, menus.ts): a Claude Code session offers cross-provider
// accounts in both its right-click Move to submenu and the reply box's account chip (`Hand off
// here`); picking one stages it directly in the conversation input with an inline confirmation
// bar, lets you type a next-step note in the reply box itself and toggle Keep open, and confirms
// on ⏎ or clicking Hand off without closing/archiving the original session.
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

  var bar = function () { return q('.ca-handoff-bar'); };

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
        var t = q('.ca-picker-title', o);
        var d = q('.ca-picker-detail', o);
        return (t && t.textContent.trim() === 'Hand off here') || (d && d.textContent.trim() === 'Hand off here');
      });
      check('the account chip menu offers Hand off here for other providers', handoffOpts.length > 0, all('.ca-picker-title', menu).map(function (t) { return t.textContent; }).join(' | '));
      handoffOpts[0].click();

      // 3. Inline confirmation bar appears inside the reply box, and the chip highlights the staged target.
      var strip = await until(bar, 3000, 'picking Hand off here showed no inline confirmation bar');
      var textEl = q('.ca-handoff-text', strip);
      check('inline bar describes the cross-provider handoff and archive fate', /^Move to /.test(textEl.textContent) && /closes this/.test(textEl.textContent) && /handoff brief/.test(textEl.title) && /full transcript/.test(textEl.title), textEl.textContent + ' | ' + textEl.title);
      check('the account chip highlights the staged target', q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip.is-handoff') !== null, chip.className);
      var input = q('.ca-peek .ca-composer-field textarea');
      check('the conversation input stays ready for an optional next-step note', !!input, 'no textarea');

      // 4. Typing a next-step note right into the reply box and toggling Keep open updates the bar and confirm button.
      var setVal = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setVal.call(input, 'Finish fixing the flaky test');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(50);
      var keepInput = q('.ca-handoff-keep input[type="checkbox"]', strip);
      check('inline bar has a Keep open checkbox', !!keepInput && !keepInput.checked, 'missing or checked');
      keepInput.click();
      await until(function () { return /keeps this open/.test(q('.ca-handoff-text', strip).textContent); }, 2000, 'bar did not switch to Keep open');
      var go = q('.ca-composer-confirm');
      check('keeping open updates bar copy and confirm button', /^Hand off to /.test(q('.ca-handoff-text', strip).textContent) && go && /^Hand off to /.test(go.textContent), q('.ca-handoff-text', strip).textContent + ' | ' + (go ? go.textContent : 'no button'));

      // 5. Pressing Enter in the conversation input confirms with keep: true and the typed prompt without hiding the peek.
      var hidesBefore = calls('peek_hide').length;
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      await until(function () { return calls('move_session').length > 0; }, 3000, 'pressing Enter asked for no move_session');
      var asked = calls('move_session')[0][1];
      check('move_session receives keep: true and the typed next-step prompt', asked.key === key && asked.keep === true && asked.prompt === 'Finish fixing the flaky test', JSON.stringify(asked));
      await until(function () { return !bar(); }, 3000, 'inline bar stayed up after Hand off');
      await wait(150);
      check('keeping the session open leaves the peek open rather than hiding it', calls('peek_hide').length === hidesBefore, String(calls('peek_hide').length - hidesBefore));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
