// Checks the panel answering what Claude Code asks as a session exits, whether to keep the
// worktree it made (`Pending.exiting`, claude/exiting.rs): its one question with its choices and
// what each does, no box for your own answer, Cancel in place of Decline (which calls the exit
// off), and Answer sending the pick keyed by the question.
//   scripts/preview.sh sessions '' scripts/preview/questions-exiting-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions-exiting) it leaves the panel open, Remove picked,
// for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var question = 'You have 1 commit on worktree-kestrel. The branch will be deleted if you remove the worktree.';
  var pending = {
    id: 'exit-worktree:4242:1794347852465',
    label: 'Exiting worktree session',
    input: null,
    cannot: null,
    questions: [
      {
        question: question,
        header: 'Exiting',
        options: [
          { label: 'Keep worktree', description: 'Stays at /Users/me/projects/northwind-app/.claude/worktrees/kestrel' },
          { label: 'Remove worktree', description: 'All changes and commits will be lost.' },
        ],
      },
    ],
    exiting: true,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Picked Remove worktree. It exited, its terminal is closed, and it’s archived.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (picked || l.hidden || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = null;
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-questions'); };
  var button = function (text) {
    return Array.prototype.filter.call(box().querySelectorAll('button'), function (b) { var l = b.querySelector('.ca-question-choice-label > span'); return (l || b).textContent.trim() === text; })[0];
  };
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var on = function (b) { return b.getAttribute('aria-checked') === 'true'; };

  say('questions-exiting check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box(); }, 5000);
    })
    .then(function () {
      var label = box().querySelector('.ca-question .ca-approval-label');
      check('its question, named', !!label && label.textContent.indexOf(question) >= 0 && /Exiting/.test(label.textContent), label ? label.textContent : 'none');
      var descs = Array.prototype.map.call(box().querySelectorAll('.ca-question-desc'), function (d) { return d.textContent; });
      check('each choice says what it does', !!button('Keep worktree') && !!button('Remove worktree') && descs.length === 2 && /Stays at/.test(descs[0]) && /will be lost/.test(descs[1]), descs.join(' | '));
      check('no box for words of your own', !box().querySelector('.ca-question-own'));
      check('cancel, not decline', !!button('Cancel') && !button('Decline'));
      var note = box().querySelector('.ca-composer-foot .ca-composer-note');
      check('says what cancel does', !!note && /Cancel keeps it open, at its prompt/.test(note.textContent), note ? note.textContent : 'none');
      button('Remove worktree').click();
      return tick();
    })
    .then(function () {
      check('the pick', on(button('Remove worktree')) && !on(button('Keep worktree')));
      if (window.__questions_screenshot) return;
      button('Answer').click();
      return until(function () { return window.__calls.some(function (c) { return c[0] === 'session_answer'; }); }, 3000).then(function () {
        var c = window.__calls.filter(function (c) { return c[0] === 'session_answer'; })[0][1];
        check('sends the pick', c.id === pending.id && (c.answers || {})[question] === 'Remove worktree', JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
