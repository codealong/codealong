// Checks moving a running session to its harness's app (board.rs, harness.rs `Opens::to_app`): a
// session that runs in a terminal on the account the Claude or Codex app is signed into has Move to
// Claude app (or Move to Codex) under its panel's Open button, apart from the ways to open it and
// saying its terminal closes, and picking it opens `s|<key>|to-app`; ⌘⏎ never picks it. Needs a
// live terminal session on the app's account.
//   scripts/preview.sh sessions '' scripts/preview/move-to-app-check.js
(function () {
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target'; }).map(function (c) { return c[1].targetId; }); };
  var s, target, row;

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  say('move-to-app check: waiting for the board');
  fetch('board.json')
    .then(function (r) { return r.json(); })
    .then(function (b) {
      // One that can move now, else one mid-turn or waiting on you.
      b.lanes.forEach(function (l) {
        l.targets.forEach(function (t) {
          if (!/\|to-app$/.test(t.id) || (target && !target.disabled)) return;
          target = t;
          s = l.sessions.filter(function (x) { return t.id === 's|' + x.key + '|to-app'; })[0];
        });
      });
      if (!s) throw new Error('no session here runs in a terminal on the Claude or Codex app’s account');
      check('its worktree lists the move apart from the ways to open it', target.group === 'move' && /^Move to (Claude app|Codex)$/.test(target.label), target.label + ' · ' + target.group);
      check('saying its terminal closes, or why it can’t move yet', target.disabled ? /(let its turn end first|still runs in .+|answer it first)$/.test(target.detail) : /^Closes it in /.test(target.detail), target.detail);
      return until(function () { return document.querySelector('.ca-bay-lanes [id^="row-"][id$="' + s.key + '"]'); }, 20000);
    })
    .then(function (r) {
      row = r;
      // Rows select on mousedown; the panel shown before is replaced by the session's.
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return until(function () { var p = q('.ca-sessinsp'); return p && p.textContent.indexOf(s.title) >= 0 && q('[aria-label="Other ways to open"]', p); }, 5000);
    })
    .then(function () {
      q('.ca-sessinsp [aria-label="Other ways to open"]').click();
      return wait(200).then(function () {
        var items = all('.ca-sessinsp .ca-openin .ca-menu [role="none"]');
        var at = items.map(function (el) { return el.querySelector('.ca-menu-label > span:first-child').textContent; }).indexOf(target.label);
        check('its panel’s Open button lists it', at > 0, items.map(function (el) { return el.textContent; }).join(' | '));
        var item = items[at];
        check('after a separator', item && !!item.querySelector('.ca-menu-sep'), item ? item.innerHTML.slice(0, 120) : 'none');
        var button = item && item.querySelector('[role="menuitem"]');
        check('enabled as the board says', button && button.disabled === !!target.disabled, button ? String(button.disabled) : 'none');
        if (button && !button.disabled) {
          button.click();
          return wait(100).then(function () {
            check('picking it moves the session', opens().pop() === target.id, opens().join());
          });
        }
      });
    })
    .then(function () {
      var before = opens().length;
      // From the list, as Return opens the row: picking the row put the keys in its reply box.
      if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
      key(document.body, { key: 'Enter', code: 'Enter', metaKey: true });
      return wait(100).then(function () {
        var now = opens().slice(before);
        check('⌘⏎ opens something else', now.length === 1 && now[0] !== target.id, now.join() || 'nothing');
      });
    })
    .then(done)
    .catch(stopped);
})();
