// Checks Ask in a PR's box (PrCard.tsx, SideStrip.tsx), in the waiting window: the box opens on
// Review; `/side` and a question there asks about the PR, puts the box on Ask and shows the
// question above it; Ask has a draft of its own, so Review's text is still there after; ⏎ on Ask
// asks the account beside it, Auto's at first, and the answer shows as the engine tells of it;
// once a conversation is open the account is said, not picked; Close ends it and keeps it, and
// "1 earlier" lists it; the agent menu beside Ask (AgentMenu.tsx) offers Auto and the accounts
// whose harness can answer, each with its models and an effort row, and the account, model and
// effort picked are asked next and remembered; a question that can't be asked is back in the box
// with why; one from before read while it's still answering says its own agent answers, and once
// it has answered, a question asked as it's read goes on in it, on what's picked rather than the
// open one's account; and ⌘⏎ on Ask approves with Review's text.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/pr-ask-check.js
(function () {
  ['account', 'model', 'effort'].forEach(function (k) { localStorage.removeItem('codealong.prask.' + k); });
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
  // The open menu's lines, by the account whose heading they're under ('' for Auto's).
  var lines = function () {
    var out = {};
    all('.ca-picker-opt').filter(function (o) { return !o.disabled; }).forEach(function (o) {
      var head = o.closest('.ca-picker-section').querySelector('.ca-quick-acct-name');
      var name = head ? head.textContent : '';
      (out[name] = out[name] || []).push(o);
    });
    return out;
  };
  var model = null;
  var effort = null;
  var firstId = null;

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
      firstId = window.__sides[pr.key].id;
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
      check('Close ends it, and who answers can be picked again: Auto, saying its account', sent('pr_side_close')[0].key === pr.key && chip().textContent.indexOf('Auto') === 0 && chip().textContent.indexOf(auto) > 0, chip() && chip().textContent);
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
      check('no native menu is asked for', menus() === 0, menus());
      chip().click();
      return until(function () { return lines().codex; }, 3000, 'the agent menu');
    })
    .then(function () {
      var l = lines();
      var names = Object.keys(l);
      check('the agent menu offers Auto, then the accounts that can answer, under their headings', l[''] && /^Auto/.test(l[''][0].textContent) && names.indexOf('codex') > 0 && names.indexOf('opencode') < 0 && names.indexOf('antigravity') < 0, JSON.stringify(names));
      // A model of codex's where it lists one, else its default.
      var line = l.codex[1] || l.codex[0];
      model = l.codex[1] ? line.querySelector('.ca-picker-title').textContent : null;
      line.click();
      return until(function () { return chip().textContent.indexOf('codex') === 0; }, 3000, 'codex picked');
    })
    .then(function () {
      check('the one picked is remembered', localStorage.getItem('codealong.prask.account') === 'codex');
      chip().click();
      return until(function () { return q('.ca-picker') ? (all('.ca-picker-foot button').filter(function (b) { return b.textContent.trim() === 'High'; })[0] || 'none') : null; }, 3000, 'the menu again');
    })
    .then(function (level) {
      if (level === 'none') return null;
      level.click();
      effort = 'high';
      return until(function () { return /High effort/.test(chip().textContent); }, 3000, 'the effort on the chip');
    })
    .then(function () {
      if (q('.ca-picker')) chip().click();
      check('its effort is remembered with it', localStorage.getItem('codealong.prask.effort') === (effort || '') || (effort === null && !localStorage.getItem('codealong.prask.effort')), localStorage.getItem('codealong.prask.effort'));
      failNext = true;
      type(box(), 'Is it safe?');
      enter();
      return until(function () { return /Could not resolve/.test((q('.ca-prreview-box .ca-composer-note') || {}).textContent || ''); }, 3000, 'the failure');
    })
    .then(function () {
      check('one that can’t be asked is back in the box, with why', box().value === 'Is it safe?' && asked().account === 'codex', box().value);
      check('it was asked on the model and effort picked', (model === null ? asked().model === null : !!asked().model) && asked().effort === effort, JSON.stringify(asked()) + ' picked ' + model);
      check('and nothing shows as starting', !q('.ca-sidestrip-doing') || q('.ca-sidestrip-doing').textContent !== 'Reading the PR…');
      // Another conversation, open on what's picked; then the first, read again and asked.
      type(box(), 'Is it safe?');
      enter();
      return until(function () { var s = (window.__sides || {})[pr.key]; return s && s.id !== firstId && !!q('.ca-prreview-who'); }, 3000, 'another conversation open');
    })
    .then(function () {
      all('.ca-sidestrip-head button').filter(function (b) { return b.getAttribute('aria-label') === 'Earlier'; })[0].click();
      return until(function () { return all('.ca-sidestrip-kept').filter(function (b) { return b.textContent.indexOf('Why a new table?') === 0; })[0]; }, 3000, 'the list');
    })
    .then(function (row) {
      row.click();
      return until(function () { return all('.ca-sidestrip-q').length === 2 && q('.ca-sidestrip-q').textContent === 'Why a new table?'; }, 3000, 'the first read again');
    })
    .then(function () {
      // Closed with its follow-up unanswered, it's still answering, on its own agent.
      check('one read while it’s still answering says its own agent answers', !chip() && ((q('.ca-prreview-who') || {}).textContent || '').indexOf(auto) === 0, (q('.ca-prreview-who') || {}).textContent);
      var first = window.__closing[firstId];
      var turns = first.turns.map(function (t) { return Object.assign({}, t, { answer: 'In one step.', done: true }); });
      delete window.__closing[firstId];
      window.__past = (window.__past || []).concat([{ id: firstId, key: pr.key, title: '', startedAt: first.startedAt, lastAt: Date.now(), turns: turns }]);
      window.__emit('side', Object.assign({}, first, { turns: turns, running: false }));
      return until(function () { return !!chip(); }, 3000, 'it answered');
    })
    .then(function () {
      check('once it has answered, who answers can be picked', !!chip() && !q('.ca-prreview-who'));
      type(box(), 'And the index?');
      enter();
      return until(function () { return asked().text === 'And the index?'; }, 3000, 'the question');
    })
    .then(function () {
      var a = asked();
      check('a question asked as one from before is read goes on in it, on what’s picked', a.from === firstId && a.account === 'codex' && (model === null ? a.model === null : !!a.model) && a.effort === effort, JSON.stringify(a) + ' first ' + firstId);
      key(window, { key: 'Enter', metaKey: true });
      return until(function () { return sent('review_pr').length === 1; }, 3000, 'the approval');
    })
    .then(function () {
      var r = sent('review_pr')[0];
      check('⌘⏎ on Ask approves with Review’s text', r.verdict === 'approve' && r.body === 'Looks good.', JSON.stringify(r));
    })
    .then(done, stopped);
})();
