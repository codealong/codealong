// Checks the panel answering a dialog of Claude Code's own read off its terminal (`Pending.dialog`,
// claude/dialog.rs), its offer to make auto mode the default: its title and what it says, its
// choices, no box for your own answer, Cancel in place of Decline (which presses Escape there),
// and Answer sending the pick keyed by the question.
//   scripts/preview.sh sessions '' scripts/preview/questions-dialog-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions-dialog) it leaves the panel open, No picked, for a
// screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var question =
    'Make auto mode your default permission mode? Auto mode lets Claude handle permission prompts automatically. Claude checks each tool call for risky actions and prompt injection before executing, runs the ones it assesses as lower-risk, and blocks the rest.';
  var pending = {
    id: 'dialog:4242:1795021185513',
    label: 'Make auto mode your default permission mode?',
    input: null,
    cannot: null,
    questions: [
      {
        question: question,
        options: [{ label: 'Yes, set auto mode as my default permission mode' }, { label: 'No, keep bypass permissions' }],
      },
    ],
    dialog: true,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Picked No, keep bypass permissions.');
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
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('questions-dialog check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box(); }, 5000);
    })
    .then(function () {
      var label = box().querySelector('.ca-question .ca-approval-label');
      check('its title and what it says', !!label && label.textContent.indexOf(question) >= 0, label ? label.textContent : 'none');
      check('its choices', !!button('Yes, set auto mode as my default permission mode') && !!button('No, keep bypass permissions'));
      check('no box for words of your own', !box().querySelector('.ca-question-own'));
      check('cancel, not decline', !!button('Cancel') && !button('Decline'));
      var note = box().querySelector('.ca-composer-foot .ca-composer-note');
      check('says what cancel does', !!note && /Cancel presses Escape there/.test(note.textContent), note ? note.textContent : 'none');
      button('No, keep bypass permissions').click();
      return tick();
    })
    .then(function () {
      check('the pick', on(button('No, keep bypass permissions')) && !on(button('Yes, set auto mode as my default permission mode')));
      if (window.__questions_screenshot) return;
      button('Answer').click();
      return until(function () { return answers().length > 0; }, 3000).then(function () {
        var c = answers()[0][1];
        check('sends the pick', c.id === pending.id && (c.answers || {})[question] === 'No, keep bypass permissions', JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
