// Checks Open in Claude app for a Claude Code session that runs in a terminal or as a background
// job (claude/mod.rs `targets`): with Remote Control it's offered under its panel's Open button
// beside the terminal's way, and picking it opens `s|<key>|claude-app`; without, it says Remote
// Control is needed. Needs a live session on the Claude app's account.
//   scripts/preview.sh sessions '' scripts/preview/claude-app-open-check.js
(function () {
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target'; }).map(function (c) { return c[1].targetId; }); };
  var s, target;

  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  say('claude-app-open check: waiting for the board');
  fetch('board.json')
    .then(function (r) { return r.json(); })
    .then(function (b) {
      // One with Remote Control, else one without.
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (x) {
          if (x.harness !== 'claude' || x.surface !== 'cli' || !x.live || x.hosted) return;
          var t = l.targets.filter(function (y) { return y.id === 's|' + x.key + '|claude-app'; })[0];
          if (!t || (target && !target.disabled)) return;
          s = x;
          target = t;
        });
      });
      if (!s) throw new Error('no session here runs in a terminal on the Claude app’s account');
      check('a running session offers the Claude app', target.label === 'Open in Claude app', target.label);
      check('as Remote Control lets it', target.disabled ? !s.remoteControl && target.detail === 'Needs Remote Control (/remote-control)' : !!s.remoteControl, target.detail + ' · ' + s.remoteControl);
      return until(function () { return document.querySelector('.ca-bay-lanes [id^="row-"][id$="' + s.key + '"]'); }, 20000);
    })
    .then(function (row) {
      // Rows select on mousedown; the panel shown before is replaced by the session's.
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return until(function () { var p = q('.ca-sessinsp'); return p && p.textContent.indexOf(s.title) >= 0 && q('[aria-label="Other ways to open"]', p); }, 5000);
    })
    .then(function () {
      q('.ca-sessinsp [aria-label="Other ways to open"]').click();
      return wait(200).then(function () {
        var items = all('.ca-sessinsp .ca-openin .ca-menu [role="menuitem"]');
        var item = items.filter(function (el) { return el.textContent.indexOf('Open in Claude app') >= 0; })[0];
        check('its panel’s Open button lists it', !!item, items.map(function (el) { return el.textContent; }).join(' | '));
        check('enabled as the board says', item && item.disabled === !!target.disabled, item ? String(item.disabled) : 'none');
        if (item && !item.disabled) {
          item.click();
          return wait(100).then(function () {
            check('picking it opens the session in the app', opens().pop() === target.id, opens().join());
          });
        }
      });
    })
    .then(done)
    .catch(stopped);
})();
