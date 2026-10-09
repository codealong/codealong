// Checks Ask in a PR's box (PrCard.tsx, SideStrip.tsx), in the waiting window: the box opens on
// Review; `/side` and a question there asks about the PR, puts the box on Ask and shows the
// question above it; Ask has a draft of its own, so Review's text is still there after; ⏎ on Ask
// asks the account beside it, Auto's at first, and the answer shows as the engine tells of it;
// once a conversation is open the account is said, not picked; Close ends it and keeps it, and
// "1 earlier" lists it; the account menu offers Auto and the accounts whose harness can answer,
// and the one picked is asked next; a question that can't be asked is back in the box with why;
// and ⌘⏎ on Ask approves with Review's text.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/pr-ask-check.js
(function () {
  localStorage.removeItem('codealong.prask.account');
  var pr = null;
  var failNext = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_side_ask' && failNext) {
      failNext = false;
      window.__calls.push([cmd, args]);
      return Promise.reject('GitHub: Could not resolve to a PullRequest.');
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

  var box = function () { return q('.ca-prreview-box textarea'); };
  var on = function () { var b = q('.ca-prreview-sides [aria-checked="true"]'); return b ? b.textContent : null; };
  var side = function (name) { return all('.ca-prreview-sides .ca-segctl button').filter(function (b) { return b.textContent === name; })[0]; };
  var chip = function () { return q('.ca-prreview-buttons .ca-modelmenu-chip'); };
  var strip = function () { return q('.ca-prreview .ca-sidestrip'); };
  var asked = function () { var c = sent('pr_side_ask'); return c[c.length - 1] || null; };
  var enter = function () { key(box(), { key: 'Enter' }); };
  var answer = function (turns, more) { window.__emit('side', Object.assign({ id: window.__sides[pr.key].id, key: pr.key, turns: turns, running: false, ended: false, account: window.__sides[pr.key].account }, more || {})); };
  var auto;

  say('pr ask check: waiting for the card');
  until(function () { return !!pr && !!box() && !!on(); }, 20000, 'the card')
    // The accounts that can answer, which the card reads as it comes.
    .then(function () { return wait(500); })
    .then(function () {
      check('the box opens on Review', on() === 'Review', on());
      check('nothing shows above the box on Review', !strip());
      type(box(), '/side Why a new table?');
      enter();
      return until(function () { return !!asked(); }, 3000, 'the question');
    })
    .then(function () {
      var a = asked();
      auto = a.account;
      check('/side asks about the PR, by its key', a.key === pr.key && a.text === 'Why a new table?', JSON.stringify(a));
      check('on an account that can answer', ['claude', 'claude2', 'codex'].indexOf(auto) >= 0, auto);
      return until(function () { return on() === 'Ask' && !!q('.ca-sidestrip-q'); }, 3000, 'Ask with the question');
    })
    .then(function () {
      check('the box is on Ask, emptied', on() === 'Ask' && box().value === '', box().value);
      check('the question shows above it', q('.ca-sidestrip-q').textContent === 'Why a new table?');
      check('the strip says what it’s about and who answers', /About #\d+/.test(q('.ca-sidestrip-title').textContent) && q('.ca-sidestrip-about').textContent.indexOf(auto + ' reads the PR') === 0, q('.ca-sidestrip-about').textContent);
      check('the open conversation’s account is said, not picked', !chip() && (q('.ca-prreview-who') || {}).textContent === auto);
      answer([{ question: 'Why a new table?', answer: 'Teams need their own rows.', done: true, at: Date.now() }]);
      return until(function () { return !!q('.ca-sidestrip-a'); }, 3000, 'the answer');
    })
    .then(function () {
      check('the answer shows as the engine tells of it', q('.ca-sidestrip-a').textContent.indexOf('Teams need their own rows.') >= 0);
      type(box(), 'And the migration?');
      enter();
      return until(function () { return sent('pr_side_ask').length === 2; }, 3000, 'the follow-up');
    })
    .then(function () {
      check('⏎ on Ask asks a follow-up', asked().text === 'And the migration?' && box().value === '', JSON.stringify(asked()));
      side('Review').click();
      return until(function () { return on() === 'Review'; }, 3000, 'Review');
    })
    .then(function () {
      check('Review puts the strip away', !strip());
      type(box(), 'Looks good.');
      side('Ask').click();
      return until(function () { return on() === 'Ask'; }, 3000, 'Ask');
    })
    .then(function () {
      check('Ask has a draft of its own, and the strip again', box().value === '' && !!strip(), box().value);
      side('Review').click();
      return until(function () { return on() === 'Review'; }, 3000, 'Review');
    })
    .then(function () {
      check('Review still has its text', box().value === 'Looks good.', box().value);
      side('Ask').click();
      return until(function () { return on() === 'Ask' && !!strip(); }, 3000, 'Ask');
    })
    .then(function () {
      var close = all('.ca-sidestrip-head button').filter(function (b) { return b.getAttribute('aria-label') === 'Close'; })[0];
      close.click();
      return until(function () { return sent('pr_side_close').length === 1 && !strip() && !!chip(); }, 3000, 'closed');
    })
    .then(function () {
      check('Close ends it, and the account can be picked again', sent('pr_side_close')[0].key === pr.key && chip().textContent === auto, chip() && chip().textContent);
      return until(function () { return all('.ca-prreview-sides .ca-linkbtn').length === 1; }, 3000, 'the earlier one');
    })
    .then(function () {
      var link = q('.ca-prreview-sides .ca-linkbtn');
      check('it says there’s one from before', link.textContent === '1 earlier', link.textContent);
      link.click();
      return until(function () { return !!q('.ca-sidestrip-kept'); }, 3000, 'the list');
    })
    .then(function () {
      check('which lists it', q('.ca-sidestrip-kept-q').textContent === 'Why a new table?' && /Questions about #/.test(q('.ca-sidestrip-title').textContent), q('.ca-sidestrip-kept-q').textContent);
      var before = menus();
      chip().click();
      return until(function () { return menus() > before; }, 3000, 'the account menu');
    })
    .then(function () {
      var labels = menuLabels();
      check('the account menu offers Auto, then the accounts that can answer', labels[0] === 'Auto (' + auto + ') ✓' && labels.indexOf('codex') > 0 && labels.indexOf('opencode') < 0 && labels.indexOf('antigravity') < 0, JSON.stringify(labels));
      menuItem('codex')();
      return until(function () { return chip().textContent === 'codex'; }, 3000, 'codex picked');
    })
    .then(function () {
      check('the one picked is remembered', localStorage.getItem('codealong.prask.account') === 'codex');
      failNext = true;
      type(box(), 'Is it safe?');
      enter();
      return until(function () { return /Could not resolve/.test((q('.ca-prreview-box .ca-composer-note') || {}).textContent || ''); }, 3000, 'the failure');
    })
    .then(function () {
      check('one that can’t be asked is back in the box, with why', box().value === 'Is it safe?' && asked().account === 'codex', box().value);
      check('and nothing shows as starting', !q('.ca-sidestrip-doing') || q('.ca-sidestrip-doing').textContent !== 'Reading the PR…');
      key(window, { key: 'Enter', metaKey: true });
      return until(function () { return sent('review_pr').length === 1; }, 3000, 'the approval');
    })
    .then(function () {
      var r = sent('review_pr')[0];
      check('⌘⏎ on Ask approves with Review’s text', r.verdict === 'approve' && r.body === 'Looks good.', JSON.stringify(r));
    })
    .then(done, stopped);
})();
