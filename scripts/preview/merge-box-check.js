// Checks a PR's merge box (MergeBox.tsx, merge.ts), on its card in the waiting window, where its
// row of buttons is right under the PR's title. A PR plainly ready to merge says so in its filled
// button, GitHub's pick for you, with a caret for the other ways and no line under the row;
// pressing it shows the commit GitHub would write for that way, which the caret's menu switches
// (a rebase writes none), the email it goes under, and whether the branch goes too. ⌘⏎ in the
// fields confirms, and doesn't approve the PR as ⌘⏎ does elsewhere on the card; it sends the head
// the box saw with what was written, and the way picked is kept for the repo. A merge GitHub
// refuses says why, the commit still there to send again; Escape puts the commit away and leaves
// the card up. Then, as the box is read again after each act: a blocked PR offers auto-merge, and
// merging past the rules once its box is ticked, and a branch behind its base Update branch, with
// how; a draft offers Ready for review; and a merged PR whose branch is still there, deleting it,
// what was done staying said once there's nothing left to do.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/merge-box-check.js
(function () {
  localStorage.removeItem('codealong.merge.methods');
  localStorage.removeItem('codealong.merge.deleteBranch');
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  // What the box reads as next, after each act that goes through.
  var shapes = [
    {},
    { mergeState: 'BLOCKED', reviewDecision: 'REVIEW_REQUIRED', canAuto: true, canBypass: true, canUpdateBranch: true },
    { draft: true, mergeState: 'BLOCKED' },
    { state: 'MERGED', mergedBy: 'ana' },
    { state: 'MERGED', mergedBy: 'ana', branchRef: null },
  ];
  var shape = 0;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'merge_box') {
      window.__calls.push([cmd, args]);
      var at = shapes[shape];
      return real(cmd, args, options).then(function (b) { return Object.assign(b, at); });
    }
    if (cmd === 'merge_pr') {
      return real(cmd, args, options).then(function (r) {
        shape = Math.min(shape + 1, shapes.length - 1);
        return r;
      });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        });
      });
      // The first PR asks for your review, and no other does.
      board.prs = (board.prs || []).map(function (x) {
        if (pr && x.key !== pr.key) return Object.assign({}, x, { reviewRequested: false });
        pr = Object.assign({}, x, { state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null });
        return pr;
      });
      return board;
    });
  };

  var box = function () { return q('.ca-merge'); };
  var says = function () { var s = q('.ca-merge-standing strong'); return s ? s.textContent : null; };
  var main = function () { return q('.ca-merge-main > .ca-btn'); };
  var caret = function () { return q('.ca-merge-main .ca-split-caret'); };
  var button = function (text) { return all('.ca-merge .ca-btn').filter(function (b) { return b.textContent === text; })[0]; };
  var title = function () { return q('.ca-merge-write .ca-field input'); };
  var message = function () { return q('.ca-merge-write textarea'); };
  var note = function () { var n = q('.ca-merge-note'); return n ? n.textContent : null; };
  var merges = function () { return sent('merge_pr'); };
  var cmdEnter = function (el) { key(el, { key: 'Enter', metaKey: true }); };
  var squashed;

  say('merge box check: waiting for the box');
  until(function () { return !!pr && !!main(); }, 20000, 'the merge box')
    .then(function () {
      check('a PR plainly ready says so in its filled button, with no line under the row', !says() && main().classList.contains('ca-btn--primary'), says());
      check('the merge buttons sit in the row under the PR’s title', !!q('.ca-prinsp > .ca-merge > .ca-insp-actions .ca-merge-main') && q('.ca-prinsp > .ca-merge').previousElementSibling === q('.ca-prinsp > header'));
      check('its button is GitHub’s pick for you, with a caret for the others', main().textContent === 'Squash and merge' && !!caret(), main().textContent);
      check('the box is read for the PR the card shows', JSON.stringify(sent('merge_box')[0]) === JSON.stringify({ repo: pr.repoSlug, number: pr.number }), JSON.stringify(sent('merge_box')[0]));
      main().click();
      return until(function () { return !!title(); }, 2000, 'the commit');
    })
    .then(function () {
      squashed = pr.title + ' (#' + pr.number + ')';
      check('pressing it shows the commit GitHub would write', title().value === squashed && message().value.indexOf('* ' + pr.title) === 0, title().value);
      check('the title has the keyboard', document.activeElement === title());
      var confirm = all('.ca-merge-write .ca-btn')[0];
      check('the confirm button says the way', confirm.textContent === 'Confirm squash and merge', confirm.textContent);
      check('the commit takes the button row’s place, caret and all', !caret());
      key(title(), { key: 'Escape' });
      return wait(50);
    })
    .then(function () {
      check('Escape puts the commit away, and leaves the card up', !title() && !!q('.ca-prreview') && !!main());
      caret().click();
      return wait(50);
    })
    .then(function () {
      check('the caret lists the three ways, the one picked checked', JSON.stringify(menuLabels()) === JSON.stringify(['Create a merge commit', 'Squash and merge ✓', 'Rebase and merge']), JSON.stringify(menuLabels()));
      menuItem('Create a merge commit')();
      return wait(50);
    })
    .then(function () {
      check('picking another way says it on the button', main().textContent === 'Merge', main().textContent);
      check('and keeps it for the repo', JSON.parse(localStorage.getItem('codealong.merge.methods') || '{}')[pr.repoSlug] === 'merge', localStorage.getItem('codealong.merge.methods'));
      main().click();
      return until(function () { return !!title(); }, 2000, 'the commit');
    })
    .then(function () {
      check('a merge commit gets GitHub’s title for one', title().value.indexOf('Merge pull request #' + pr.number) === 0 && message().value === pr.title, title().value);
      key(title(), { key: 'Escape' });
      return wait(50);
    })
    .then(function () {
      caret().click();
      return wait(50);
    })
    .then(function () {
      menuItem('Rebase and merge')();
      return wait(50);
    })
    .then(function () {
      main().click();
      return wait(100);
    })
    .then(function () {
      check('a rebase writes no commit of its own', !title() && !message() && !!q('.ca-merge-write') && /one by one/.test(q('.ca-merge-write').textContent), q('.ca-merge-write') && q('.ca-merge-write').textContent);
      all('.ca-merge-write .ca-btn').filter(function (b) { return b.textContent === 'Cancel'; })[0].click();
      return wait(50);
    })
    .then(function () {
      caret().click();
      return wait(50);
    })
    .then(function () {
      menuItem('Squash and merge')();
      return wait(50);
    })
    .then(function () {
      main().click();
      return until(function () { return !!title(); }, 2000, 'the commit');
    })
    .then(function () {
      check('back on squash, its commit again', title().value === squashed, title().value);
      var chip = q('.ca-merge-opts .ca-modelmenu-chip');
      check('the email it goes under is GitHub’s pick', !!chip && chip.textContent === 'you@example.com', chip && chip.textContent);
      chip.click();
      return wait(50);
    })
    .then(function () {
      check('the email menu lists yours', JSON.stringify(menuLabels()) === JSON.stringify(['you@example.com ✓', 'you@work.example']), JSON.stringify(menuLabels()));
      menuItem('you@work.example')();
      return wait(50);
    })
    .then(function () {
      var del = q('.ca-merge-opts .ca-check input');
      check('deleting the branch is offered, off at first', !!del && !del.checked && q('.ca-merge-opts .ca-check').textContent === 'Delete ' + pr.head + ' once merged', q('.ca-merge-opts .ca-check') && q('.ca-merge-opts .ca-check').textContent);
      del.click();
      type(title(), 'fail');
      cmdEnter(title());
      return until(function () { return !!note(); }, 2000, 'the refusal');
    })
    .then(function () {
      check('a merge GitHub refuses says why', /Head branch was modified/.test(note()) && q('.ca-merge-note').classList.contains('is-error'), note());
      check('and the commit is still there to send again', !!title() && title().value === 'fail');
      check('⌘⏎ in the commit didn’t approve the PR', calls('review_pr').length === 0, calls('review_pr').length);
      type(title(), 'Ship teams (#' + pr.number + ')');
      cmdEnter(title());
      return until(function () { return merges().length === 2 && shape === 1 && says() === 'Merging is blocked'; }, 3000, 'the merge, and the box read again');
    })
    .then(function () {
      var m = merges()[1];
      check('it sends the head it saw, with what was written', m.number === pr.number && m.id === 'PR_' + pr.number && m.op.op === 'merge' && m.op.method === 'squash' && m.op.head === '0ef4ed2' && m.op.title === 'Ship teams (#' + pr.number + ')' && m.op.body === message0() && m.op.email === 'you@work.example', JSON.stringify(m));
      check('and the branch to delete', m.op.deleteBranch === 'REF_' + pr.number, m.op.deleteBranch);
      check('it says what was done', note() === 'Squashed and merged #' + pr.number + ' and deleted ' + pr.head + '.', note());
      check('the choice to delete is kept', localStorage.getItem('codealong.merge.deleteBranch') === '1');
      check('a blocked PR says why', /needs an approving review/.test(q('.ca-merge-standing span').textContent), q('.ca-merge-standing span').textContent);
      check('and offers auto-merge', main().textContent === 'Enable auto-merge', main().textContent);
      var bypass = q('.ca-merge-bypass input');
      check('and merging past the rules, unticked', !!bypass && !bypass.checked);
      bypass.click();
      return wait(50);
    })
    .then(function () {
      check('ticked, the button merges past them', main().textContent === 'Bypass rules and merge' && main().classList.contains('is-bypass'), main().textContent);
      main().click();
      return until(function () { return !!title(); }, 2000, 'the commit');
    })
    .then(function () {
      check('past the rules, the commit warns', /past the repo’s rules/.test(q('.ca-merge-warn').textContent));
      all('.ca-merge-write .ca-btn').filter(function (b) { return b.textContent === 'Cancel'; })[0].click();
      return wait(50);
    })
    .then(function () {
      var update = button('Update branch');
      check('a branch behind can be brought up to date', !!update);
      var how = update && update.nextElementSibling;
      how.click();
      return wait(50);
    })
    .then(function () {
      check('by a merge commit or a rebase', JSON.stringify(menuLabels()) === JSON.stringify(['Update with a merge commit', 'Update with a rebase']), JSON.stringify(menuLabels()));
      menuItem('Update with a rebase')();
      return until(function () { return merges().length === 3 && says() === 'Still a draft'; }, 3000, 'the update, and the box read again');
    })
    .then(function () {
      check('the update names the head it saw', JSON.stringify(merges()[2].op) === JSON.stringify({ op: 'update-branch', rebase: true, head: '0ef4ed2' }), JSON.stringify(merges()[2].op));
      check('a draft offers Ready for review, and no merge', !!button('Ready for review') && !main(), main() && main().textContent);
      button('Ready for review').click();
      return until(function () { return merges().length === 4 && /^Merged by ana$/.test(says() || ''); }, 3000, 'ready, and the box read again');
    })
    .then(function () {
      check('ready for review asks for it', merges()[3].op.op === 'ready');
      var del = button('Delete ' + pr.head);
      check('a merged PR offers deleting its branch', !!del);
      del.click();
      return until(function () { return merges().length === 5 && note() === 'Deleted ' + pr.head + '.'; }, 3000, 'the branch deleted');
    })
    .then(function () {
      check('deleting it names its ref', JSON.stringify(merges()[4].op) === JSON.stringify({ op: 'delete-branch', branchRef: 'REF_' + pr.number }), JSON.stringify(merges()[4].op));
      check('what was done stays said once there’s nothing left to do', !!box() && !button('Delete ' + pr.head));
    })
    .then(done, stopped);

  // The message GitHub would write for a squash, as the stub has it.
  function message0() { return '* ' + pr.title + '\n\n* Say what it does in the docs'; }
})();
