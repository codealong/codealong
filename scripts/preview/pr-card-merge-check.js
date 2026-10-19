// Checks a PR's chip in a session's header, which says where the PR stands as the pills on its
// card and in its header lead with it (prStatus.ts `chipWord`), and the card (PrLink.tsx): it has
// no Show in Codealong, which a click on the chip does, and an open PR's card ends in GitHub's
// merge box (MergeBox.tsx), read as the card shows and once for it, its button GitHub's pick for
// you with Open on GitHub after it in the row. Left untouched, the card goes as the pointer leaves it.
// Pressing the merge button shows the commit on the card, which stays up from then on as the
// pointer leaves and stays inside the window as it grows; keys typed into the commit don't put it
// away, and Escape there puts the commit away and leaves the card. ⌘⏎ merges the PR with what was
// written, and the card stays, saying so; a key typed elsewhere then puts it away. The session's
// PR is made an open one that's someone else's, whatever it is today.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-card-merge-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', '1d');
  localStorage.removeItem('codealong.merge.methods');
  // The first session, in its own worktree, that a PR lists.
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || s.visitor || s.archived || l.hidden) return;
          var pr = board.prs.find(function (x) { return x.sessions.some(function (y) { return y.key === s.key; }); });
          if (pr) picked = { key: s.key, lane: l.id, title: s.title, pr: pr };
        });
      });
      if (picked) {
        board.prs.forEach(function (x) {
          if (x.key === picked.pr.key) Object.assign(x, { state: 'open', mine: false, author: 'ana', reviewRequested: false, snoozedUntil: null, mergedAt: null, closedAt: null });
        });
        board.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) { if (s.key === picked.key) s.lastActivity = Math.max(s.lastActivity, Date.now() - 60e3); });
        });
      }
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = q('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var chip = function () { return all('.ca-sessinsp .ca-prchipcard').filter(function (el) { return el.textContent.indexOf('#' + picked.pr.number) >= 0; })[0]; };
  var card = function () { return q('.ca-prhover'); };
  var main = function () { return card() && q('.ca-merge-main > .ca-btn', card()); };
  var commit = function () { return card() && q('.ca-merge-write .ca-field input', card()); };
  var note = function () { var n = card() && q('.ca-merge-note', card()); return n ? n.textContent : null; };
  var labels = function () { return all('button', card()).map(function (b) { return b.textContent.trim(); }); };
  var hover = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var inWindow = function () { var r = card().getBoundingClientRect(); return r.top >= 0 && r.bottom <= window.innerHeight; };
  var squashed;

  say('pr card merge check: waiting for the list');
  until(function () { return row(); }, 20000)
    .catch(function () { throw new Error('no session on the board that a PR lists'); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return title() === picked.title && chip(); }, 10000, 'the session’s panel, with its PR’s chip');
    })
    .then(function () {
      hover(chip());
      return until(function () { return !!main(); }, 5000, 'the card’s merge box');
    })
    .then(function () {
      squashed = picked.pr.title + ' (#' + picked.pr.number + ')';
      var word = q('.ca-pr-word', chip());
      var pill = q('.ca-prhover-head .ca-prpill-text, .ca-hovercard-head .ca-prpill-text', card());
      check('the chip says where the PR stands, as its pills lead with it', !!word && !!pill && word.textContent === pill.textContent, (word && word.textContent) + ' / ' + (pill && pill.textContent));
      check('the chip’s card has no Show in Codealong', labels().indexOf('Show in Codealong') < 0, labels().join(' | '));
      check('it has GitHub’s merge box, its button GitHub’s pick for you', main().textContent === 'Squash and merge', main().textContent);
      var row = q('.ca-prhover-merge .ca-insp-actions', card());
      var buttons = row ? all(':scope > .ca-btn, :scope > .ca-split', row) : [];
      var last = buttons[buttons.length - 1];
      check('with Open on GitHub after it in its row', buttons.length >= 2 && buttons[0].classList.contains('ca-merge-main') && !!last && last.textContent === 'Open on GitHub', buttons.map(function (b) { return b.textContent; }).join(' | '));
      check('the box is read for the PR', sent('merge_box').some(function (a) { return a.repo === picked.pr.repoSlug && a.number === picked.pr.number; }), JSON.stringify(sent('merge_box')));
      // Into the card and out of it, nothing pressed.
      chip().dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: card() }));
      leave(card());
      return until(function () { return !card(); }, 2000, 'the card to go as the pointer leaves it');
    })
    .then(function () {
      check('left untouched, it goes as the pointer leaves', true);
      hover(chip());
      return until(function () { return !!main(); }, 5000, 'the card again');
    })
    .then(function () {
      check('its box is read once for the PR', sent('merge_box').length === 1, String(sent('merge_box').length));
      press(main());
      return until(function () { return !!commit(); }, 2000, 'the commit');
    })
    .then(function () {
      check('pressing it shows the commit GitHub would write, on the card', commit().value === squashed, commit().value);
      check('which has the keyboard', document.activeElement === commit());
      leave(card());
      return wait(500);
    })
    .then(function () {
      check('pressed, the card stays up as the pointer leaves', !!card());
      check('and inside the window as it grows', inWindow(), JSON.stringify(card().getBoundingClientRect()));
      key(commit(), { key: 'a' });
      return wait(50);
    })
    .then(function () {
      check('a key typed into the commit leaves it up', !!card() && !!commit());
      key(commit(), { key: 'Escape' });
      return wait(50);
    })
    .then(function () {
      check('Escape there puts the commit away and leaves the card', !!card() && !commit() && !!main());
      press(main());
      return until(function () { return !!commit(); }, 2000, 'the commit again');
    })
    .then(function () {
      type(commit(), 'Ship the card’s merge');
      key(commit(), { key: 'Enter', metaKey: true });
      return until(function () { return note() !== null; }, 3000, 'what the merge did');
    })
    .then(function () {
      var m = sent('merge_pr')[0];
      check('⌘⏎ merges it with what was written', sent('merge_pr').length === 1 && m.op.op === 'merge' && m.op.method === 'squash' && m.op.title === 'Ship the card’s merge', JSON.stringify(sent('merge_pr')));
      check('and the card stays, saying so', !!card() && /#\d+/.test(note()), note());
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }));
      return until(function () { return !card(); }, 1000, 'the card to go on a key');
    })
    .then(function () {
      check('a key typed elsewhere then puts it away', true);
      done();
    })
    .catch(stopped);
})();
