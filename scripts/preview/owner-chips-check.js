// Checks the header's chip for the session that owns a todo this one is on too (PeerTalkBits.tsx
// `OtherOwnerChips`): what it owns says itself whole, on the chip's one line, and the owner's long
// title is the part cut short, inside the chip; its tooltip says who, and a click shows the owner.
//   scripts/preview.sh sessions '' scripts/preview/owner-chips-check.js
(function () {
  // This runs before the app's script: an open todo whose owner is listed on this Mac is given a
  // second session of the owner's worktree, and the owner a title too long for a chip.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var LONG = 'Export invoices as CSV for the month-end close, behind a flag until finance has tried it';
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var pinned = board.pinned || [];
      var listed = function (l, s) { return !l.hidden && !l.device && !s.visitor && !s.archived && pinned.indexOf(s.key) < 0; };
      (board.todos || []).some(function (t) {
        if (!t.primary || t.status === 'done' || t.archivedAt) return false;
        return board.lanes.some(function (l) {
          var owner = l.sessions.find(function (s) { return s.key === t.primary.session && listed(l, s); });
          var other = owner && l.sessions.find(function (s) { return s.key !== owner.key && listed(l, s) && t.sessions.indexOf(s.key) < 0; });
          if (!other) return false;
          // Its own todos are this one alone, so the header's todo is it.
          board.todos.forEach(function (u) { u.sessions = u.sessions.filter(function (k) { return k !== other.key; }); });
          t.sessions = t.sessions.concat([other.key]);
          owner.title = LONG;
          picked = { lane: l.id, key: other.key, title: other.title, owner: owner.key };
          return true;
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function () { return document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var chip = function () { return document.querySelector('.ca-sessinsp-links .ca-ownerchip'); };

  say('owner chips check: waiting for the list');
  until(function () { return !!picked && !!row(); }, 20000, 'an open todo whose owner shares its worktree with another session')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === picked.title && !!chip(); }, 3000, 'the session shown, with the owner’s chip');
    })
    .then(function () { return wait(100); })
    .then(function () {
      var c = chip();
      var what = c.children[0];
      var name = c.querySelector('.ca-chipname');
      var links = c.parentElement.getBoundingClientRect();
      var box = c.getBoundingClientRect();
      check('one chip, for the todo', document.querySelectorAll('.ca-sessinsp-links .ca-ownerchip').length === 1);
      check('it says what it owns, then the owner', what.textContent === 'Todo owner:' && !!name && name.textContent === LONG, c.textContent);
      check('on one line, a chip’s height', box.height <= 22 && what.getBoundingClientRect().height <= box.height, box.height + ' / ' + what.getBoundingClientRect().height);
      check('what it owns is whole', what.scrollWidth <= what.clientWidth + 1, what.scrollWidth + ' > ' + what.clientWidth);
      check('the owner’s title is cut short, inside the chip', name.scrollWidth > name.clientWidth && name.getBoundingClientRect().right <= box.right + 0.5, name.scrollWidth + ' / ' + name.clientWidth);
      check('within the header’s width', box.right <= links.right + 0.5, box.right + ' > ' + links.right);
      check('its tooltip says who owns it, and what a click does', c.title === 'Todo is owned by “' + LONG + '”. Click to open the owning session.', c.title);
      c.click();
      return until(function () { return title() === LONG; }, 3000, 'the owner shown');
    })
    .then(function () { check('a click shows the owner', title() === LONG, title()); })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
