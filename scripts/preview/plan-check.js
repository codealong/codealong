// Checks the panel for a plan to approve (acp.rs `pending`, `Pending.plan`): a session Codealong
// runs waits on its plan, which reads as Markdown rather than as a command, under a heading that
// says it's a plan; Always says on hover what it does, in its agent's words, and answers `always`.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/plan-check.js
// As PREVIEW_INIT, after a line setting `window.__plan_screenshot = true`, it leaves the plan
// waiting for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var rule = "Approves the plan and keeps the conversation, as its agent's “Yes, and use auto mode”.";
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'toolu_1', label: 'Plan', input: '# Fix the build\n\n1. Pin the toolchain\n2. Run the tests', cannot: null, always: true, plan: true, alwaysRule: rule });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Approved the plan: “Yes, and use auto mode”.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = true;
          s.excerpt = 'Plan';
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var button = function (text) {
    // A button's text is its label, then any keys it says (Deny⌘⌫).
    var a = approval();
    return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0];
  };
  var answered = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('plan check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Always'); }, 5000);
    })
    .then(function () {
      var a = approval();
      var head = a.querySelector('.ca-approval-head').textContent;
      check('says it is a plan', head.indexOf('Wants you to approve its plan') >= 0, head);
      var plan = a.querySelector('.ca-approval-plan');
      check('the plan reads as Markdown', !!plan && !!plan.querySelector('.ca-md-h') && plan.querySelectorAll('li').length === 2, plan && plan.innerHTML);
      check('not as a command', !a.querySelector('.ca-approval-input') && !a.querySelector('.ca-approval-label'));
      check('allow, always and deny', !!button('Allow') && !!button('Always') && !!button('Deny'));
      check('always says what it does', button('Always').getAttribute('title') === rule, button('Always').getAttribute('title'));
      var note = a.querySelector('.ca-composer-note').textContent;
      check('deny keeps it planning', note.indexOf('Deny keeps it planning') >= 0, note);
      if (window.__plan_screenshot) return;
      button('Always').click();
      return until(function () { return answered().length > 0; }, 3000).then(function () {
        var c = answered()[0][1];
        check('always answers always', c.answer === 'always' && c.id === 'toolu_1' && c.key === picked.key, JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
