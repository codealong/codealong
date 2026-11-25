// Checks moving a session to another account inline in the reply box (SessionAgent.tsx,
// HandoffStrip.tsx), in the bar's peek: the chip at the right of its box's foot, its account
// first, lists its harness's other accounts under its own, and picking one stages an inline
// confirmation bar above the input (no modal), which Cancel clears without moving it; a usage
// limit with no account said to have room offers Move… in its header's box, which stages the
// first other account in the reply box and focuses it so ⏎ (or the Move button) asks for
// move_session with that account.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/move-account-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };

  // A Claude Code session on this Mac that resumes in a terminal, on an account with another here.
  var board = get('./board.json');
  var accounts = get('./accounts.json').filter(function (a) { return a.harness === 'claude' && a.signedIn && !a.disabled; });
  var picked = null;
  var others = [];
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || l.device || s.archived || s.visitor || s.harness !== 'claude' || !s.resumeCommand) return;
      var rest = accounts.filter(function (a) { return a.name !== s.account; }).map(function (a) { return a.name; });
      if (rest.length > 0) { picked = s; others = rest; }
    });
  });
  if (!picked) { stopped('no Claude Code session with another signed-in Claude Code account to move to'); return; }
  var key = picked.key;
  var to = others[0];
  localStorage.setItem('codealong.preview.peek', key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'move_session') {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.resolve({ key: 'claude:' + to + ':f0000000-0000-4000-8000-000000000000', message: 'Moved “x” to ' + to + ': it carries on in a new terminal in y.' });
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.key !== key) return;
          s.status = 'stopped';
          s.live = false;
          s.limit = { meter: 'week', resetsAt: Date.now() + 3 * 3600 * 1000, spendCapped: true, at: Date.now() - 60000, moveTo: null };
        });
      });
      return b;
    });
  };
  var bar = function () { return q('.ca-handoff-bar'); };

  (async function () {
    try {
      var chip = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); }, 20000, 'no account chip in the box');
      check('the box’s chip says its account first', chip.textContent.indexOf(picked.account) === 0, chip.textContent);
      chip.click();
      var menu = await until(function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu .ca-picker'); }, 3000, 'no menu on the chip');
      var heads = all('.ca-quick-acct-name', menu).map(function (n) { return n.textContent; });
      var moving = all('.ca-picker-opt', menu).filter(function (o) {
        var t = q('.ca-picker-title', o);
        var d = q('.ca-picker-detail', o);
        return (t && t.textContent.trim() === 'Move it here') || (d && d.textContent.trim() === 'Move it here');
      });
      check('it lists the others it can move to', others.every(function (o) { return heads.indexOf(o) >= 0; }) && moving.length >= others.length, heads.join(', ') + ' · ' + moving.length);
      check('its own account, where it has lines, comes first', heads.indexOf(picked.account) <= 0, heads.join(', '));
      moving[0].click();
      var strip = await until(bar, 3000, 'picking another account showed no inline confirmation bar');
      var summary = q('.ca-handoff-text', strip).textContent;
      var lede = q('.ca-handoff-text', strip).title;
      check('the inline bar says where it goes and what happens to this one', lede.indexOf('under ' + others[0]) >= 0 && /closed and archived/.test(lede), summary + ' | ' + lede);
      check('it tells the copy to carry on, a limit having stopped it', /told to carry on where it stopped/.test(lede), lede);
      var cancel = q('.ca-handoff-cancel', strip);
      cancel.click();
      await until(function () { return !bar(); }, 3000, 'Cancel left the inline bar up');
      check('Cancel moves nothing', calls('move_session').length === 0, String(calls('move_session').length));

      var box = await until(function () { return q('.ca-peek .ca-sessinsp-limit'); }, 3000, 'no limit box');
      var move = all('button', box).filter(function (b) { return b.textContent === 'Move…'; })[0];
      check('with no account said to have room, the limit box offers Move…', !!move, box.textContent);
      if (!move) return done();
      move.click();
      strip = await until(bar, 3000, 'Move… did not stage the move in the reply box');
      var go = q('.ca-composer-confirm');
      check('it stages the first other account with a confirm button', go && go.textContent === 'Move to ' + to, go ? go.textContent : 'no button');
      var input = q('.ca-peek .ca-composer-field textarea');
      check('the reply box takes focus so ⏎ moves it or lets you add a note', document.activeElement === input, document.activeElement ? document.activeElement.outerHTML.slice(0, 80) : 'nothing');
      go.click();
      await until(function () { return calls('move_session').length > 0; }, 3000, 'Move asked for no move');
      var asked = calls('move_session')[0][1];
      check('Move asks to move it to that account', asked.key === key && asked.account === to, JSON.stringify(asked));
      await until(function () { return !bar(); }, 3000, 'the inline bar stayed up after Move');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
