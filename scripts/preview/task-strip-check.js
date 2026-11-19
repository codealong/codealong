// Checks the strip over every panel in the main window (SessionPanel.tsx `QueueStrip`, MainPanel.tsx)
// and its Next, which goes through what waits on you in the view shown (useViewQueues.ts): a
// todo's panel has Start, Done, Snooze and Due… (TodoStrip.tsx), ⌘] goes from the todo shown to
// the next due one, and ⇧⌘E marks the one shown done and goes on, to the ready one once none due
// is left; a PR's panel has Next too, ⌘] going on to the next PR waiting on your review; once a
// view's own are through, Next offers the next kind in your order that waits in another view, and
// goes there (the todos' to PRs here, reviews being put first); a schedule's panel has Run now,
// Pause and Edit, when it runs next at the strip's start, and its Next dimmed; a worktree's has
// New session, Terminal, Editor and GitHub, and its Next goes to the first session that waits.
// Each of those three strips is flush with its panel's top edge, over nothing of the header, and
// stays there as the panel scrolls. Two open PRs are made ones waiting on your review, and the
// sample todos stand in for yours.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json PREVIEW_SIZE=1920,1100 PREVIEW_WAIT=90000 scripts/preview.sh todo '' scripts/preview/task-strip-check.js
//   PREVIEW_PLATFORM=linux PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json PREVIEW_SIZE=1920,1100 PREVIEW_WAIT=90000 scripts/preview.sh todo '' scripts/preview/task-strip-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  var asked = [];
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      // The first two open PRs wait on your review, someone else's; no other does.
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; });
      open.forEach(function (pr, i) {
        Object.assign(pr, i < 2 ? { state: 'open', mine: false, author: 'ana', reviewRequested: true, snoozedUntil: null, autoReview: null, requestedAt: Date.now() - (i + 1) * 3600e3 } : { reviewRequested: false, waitsToMerge: false });
      });
      asked = open.slice(0, 2).map(function (pr) { return pr.key; });
      // Reviews first in your order, so what Todo offers once its own are through is PRs', with no
      // session moved to the top, which would go ahead of them, nor a PR of a repo that puts its
      // reviews first.
      board.queue = { order: ['reviews', 'needs', 'due', 'unread', 'ready'], due_within: 0 };
      board.lanes.forEach(function (l) { l.sessions.forEach(function (x) { x.raised = undefined; }); });
      board.prs.forEach(function (pr) { pr.reviewFirst = false; });
      return board;
    });
  };
  // ⌘ is Ctrl on Linux and Windows (platform.ts).
  var mac = !window.__CODEALONG_PLATFORM__;
  var cmd = mac ? { metaKey: true } : { ctrlKey: true };
  var press = function (init) {
    if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, cmd, init)));
  };
  var next = function () { press({ key: ']', code: 'BracketRight' }); };
  var view = function (letter) { press({ key: letter, code: 'Key' + letter.toUpperCase(), shiftKey: true }); };
  var click = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var selected = function () { var r = q('[role="option"][aria-selected="true"]'); return r ? r.id : null; };
  var strip = function (panel) { return q(panel + ' > .ca-reply-queue'); };
  var buttons = function (s) { return all('.ca-hotkeys > .ca-hotkey:not(.is-more)', s); };
  // A button's words, without its keys (SessionPanel.tsx `Hotkey`: its words are its plain span).
  var words = function (s) { return buttons(s).map(function (b) { var w = b.querySelector(':scope > span:not([class])'); return w ? w.textContent.trim() : ''; }).filter(Boolean).join('|'); };
  var nextButton = function (s) { return buttons(s).filter(function (b) { var w = b.querySelector(':scope > span'); return w && w.textContent === 'Next'; })[0]; };
  var sessionTitle = '';
  // The sample todos due (todos-sample.json), and the one ready to start.
  var TITLES = { 't-sample1': 'Keep the sidebar', 't-sample2': 'Show how long', 't-sample4': 'Write up the config' };
  var first = null;
  var other = null;
  var said = function (s) { var n = q('.ca-reply-queue-note', s); return n ? n.textContent : ''; };
  var title = function () { var t = q('.ca-todoinsp .ca-todoinsp-title'); return t ? t.value : ''; };
  // Where a panel's strip sits: its top against the panel's own (inside its border), and what
  // comes after it below it, not under it. Scrolled, a panel held short enough to scroll.
  var placed = function (panel, scrolled) {
    var p = q(panel);
    var s = strip(panel);
    if (scrolled) { p.style.maxHeight = '160px'; p.scrollTop = 60; }
    var edge = p.getBoundingClientRect().top + p.clientTop;
    var at = s.getBoundingClientRect();
    var after = s.nextElementSibling.getBoundingClientRect();
    var moved = p.scrollTop;
    if (scrolled) { p.scrollTop = 0; p.style.maxHeight = ''; }
    var flush = Math.abs(at.top - edge) < 1;
    return { ok: scrolled ? flush && moved > 0 : flush && after.top >= at.bottom - 0.5, says: 'strip at ' + at.top + '–' + at.bottom + ', panel at ' + edge + ', what follows at ' + after.top + (scrolled ? ', scrolled ' + moved : '') };
  };
  var flush = function (panel, what) {
    var at = placed(panel, false);
    check(what + '’s strip is flush with its panel’s top, over none of the header', at.ok, at.says);
    var scrolled = placed(panel, true);
    check(what + '’s strip stays at the top as its panel scrolls', scrolled.ok, scrolled.says);
  };

  say('task strip check: the Todo view');
  until(function () { return strip('.ca-todoinsp'); }, 20000, 'the strip over a todo’s panel')
    .then(function (s) {
      var bar = q('.ca-hotkeys', s);
      check('a todo’s strip has its actions and Next', words(s) === 'Start|Done|Snooze|Due…|Next', words(s));
      check('it says what it’s for', bar.getAttribute('aria-label') === 'What’s commonly done to this todo', bar.getAttribute('aria-label'));
      flush('.ca-todoinsp', 'a todo');
      check('Start and Done say their keys', /⌘N|Ctrl\+N/.test(buttons(s)[0].textContent) && /⇧⌘E|Ctrl\+Shift\+E/.test(buttons(s)[1].textContent), buttons(s).map(function (b) { return b.textContent; }).join(' / '));
      // The two due todos are the in-progress ones, listed first: which of them is first goes by how
      // the sessions on them stand on this Mac. The other one and the one ready to start wait after it.
      first = (selected() || '').replace('row-todo:', '');
      other = first === 't-sample1' ? 't-sample2' : 't-sample1';
      check('it shows a todo due first', first === 't-sample1' || first === 't-sample2', first);
      check('it says what’s next among the todos', said(s).indexOf('1 more waiting · next: ' + TITLES[other]) === 0 && /then 1 ready todo$/.test(said(s)), said(s));
      next();
      return until(function () { return title().indexOf(TITLES[other]) === 0 && title(); }, 3000, '⌘] on to the other todo due');
    })
    .then(function () {
      check('⌘] goes on to the other todo due, selected in the list', selected() === 'row-todo:' + other, selected());
      // Done with it, the strip goes on: back round to the one skipped, which still waits.
      press({ key: 'E', code: 'KeyE', shiftKey: true });
      return until(function () { return title().indexOf(TITLES[first]) === 0 && title(); }, 3000, '⇧⌘E going on to the todo skipped');
    })
    .then(function () {
      var op = sent('todo_op').filter(function (a) { return a.op.id === other; })[0];
      check('⇧⌘E marks the todo shown done', !!op && op.op.patch.status === 'done', JSON.stringify(op));
      check('and goes on to the one skipped, which still waits', selected() === 'row-todo:' + first, selected());
      press({ key: 'E', code: 'KeyE', shiftKey: true });
      return until(function () { return title().indexOf(TITLES['t-sample4']) === 0 && title(); }, 3000, '⇧⌘E going on to the ready todo');
    })
    .then(function () {
      check('with none due left, it goes on to the one ready to start', selected() === 'row-todo:t-sample4', selected());
      var s = strip('.ca-todoinsp');
      check('with the todos through, it offers the next kind in your order that waits elsewhere', said(s) === 'Nothing else waits in Todo · next: 2 reviews in PRs', said(s));
      say('task strip check: PRs');
      nextButton(s).click();
      return until(function () { return /^row-pr:/.test(selected() || '') && strip('.ca-prpanel'); }, 3000, 'Next going to PRs, on the first that waits');
    })
    .then(function (s) {
      check('a PR’s strip has Next', /Next/.test(words(s)) && q('.ca-hotkeys', s).getAttribute('aria-label') === 'What’s commonly done to this PR', words(s));
      check('Next went to the PR that has waited longest', selected() === 'row-pr:' + asked[1], selected());
      check('it names the other PR waiting', /1 more waiting · next: #/.test(said(s)), said(s));
      var was = selected();
      next();
      return until(function () { return selected() !== was && selected(); }, 3000, '⌘] on to the other PR');
    })
    .then(function (row) {
      check('⌘] goes on to the other PR waiting on your review', asked.some(function (k) { return row === 'row-pr:' + k; }), row);
      say('task strip check: Schedules');
      view('l');
      return until(function () { return strip('.ca-schedinsp'); }, 5000, 'the strip over a schedule’s panel');
    })
    .then(function (s) {
      check('a schedule’s strip has Run now, Pause or Resume, Edit and Next', /^Run now\|(Pause|Resume)\|Edit\|Next$/.test(words(s)), words(s));
      var n = nextButton(s);
      check('its Next is dimmed, saying why', n.disabled && /Schedules don’t wait on you/.test(n.title), n.title);
      check('and its start says when the schedule runs next', /^(Next run |Next look |Due now|Looks now|Paused|Finished|Can’t run|Runs when)/.test(said(s)), said(s));
      var role = q('.ca-reply-queue-note', s).getAttribute('role');
      check('read where it stands, not announced each minute as it counts down', !role, role);
      flush('.ca-schedinsp', 'a schedule');
      say('task strip check: a worktree');
      view('s');
      return until(function () { return q('.ca-sessinsp .ca-sessinsp-lane'); }, 5000, 'a session’s panel with its worktree');
    })
    .then(function (lane) {
      sessionTitle = (q('.ca-sessinsp .ca-sessinsp-title') || {}).textContent || '';
      click(lane);
      return until(function () { return strip('.ca-laneinsp'); }, 3000, 'the worktree’s panel in the session’s place');
    })
    .then(function (s) {
      check('a worktree’s strip has its actions and Next', words(s) === 'New session|Terminal|Editor|GitHub|Next', words(s));
      flush('.ca-laneinsp', 'a worktree');
      var back = q('.ca-panelback.is-strip', s);
      check('the way back to the session is at the strip’s start, not over the header', !!back && !q('.ca-laneinsp header .ca-panelback') && s.firstElementChild === back && back.textContent.indexOf(sessionTitle.slice(0, 12)) >= 0, back ? back.textContent : 'none');
      var n = nextButton(s);
      if (n.disabled) {
        note('no session waits on you here, so Next is dimmed: ' + n.title);
        return null;
      }
      n.click();
      return until(function () { return /::/.test(selected() || '') && q('.ca-sessinsp'); }, 3000, 'Next going to the first session that waits');
    })
    .then(function () { done(); })
    .catch(stopped);
})();
