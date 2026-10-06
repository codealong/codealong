// Checks ⌘⏎ in the waiting window while a session asks you questions (SessionReply.tsx
// `Questions`): from the message box below with words in it, it answers nothing, so words left
// half typed in a question's own box aren't sent as its answer; with that box empty, it answers
// with your pick, as from anywhere in the window.
//   PREVIEW_WAIT=30000 scripts/preview.sh waiting '' scripts/preview/questions-keys-check.js
(function () {
  var picked = null;
  var pending = {
    id: 'ask-9',
    label: 'Asks you · How should I handle the evidence?',
    input: null,
    cannot: null,
    questions: [{ question: 'How should I handle the evidence?', header: 'Evidence', options: [{ label: 'Commit it' }, { label: 'Leave it' }] }],
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Answered.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked ? s.key !== picked : l.hidden || l.device || s.visitor || s.archived) return;
          picked = s.key;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = pending.label;
          s.title = 'Evidence question';
        });
      });
      // Nothing else waits: the one session is the whole queue.
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var own = function () { return q('.ca-question-own'); };
  var box = function () { return q('.ca-composer textarea'); };
  var answers = function () { return sent('session_answer'); };
  var cmdEnter = function (el) { key(el, { key: 'Enter', code: 'Enter', metaKey: true }); };
  var choice = function (text) {
    return all('.ca-question-choices button').filter(function (b) { return b.textContent.trim() === text; })[0];
  };

  say('questions keys check: waiting for the question');
  until(function () { return own() && box(); }, 20000)
    .then(function () {
      // Words left in the question's own box, then a message written below and ⌘⏎ there.
      type(own(), 'Can you expl');
      box().focus();
      type(box(), 'Can you explain what these records are in more detail?');
      return wait(50);
    })
    .then(function () {
      cmdEnter(box());
      return wait(300);
    })
    .then(function () {
      check('⌘⏎ from the message box with words answers nothing', answers().length === 0, JSON.stringify(answers()));
      check('the message stays in its box', /records are in more detail/.test(box().value), box().value);
      // Emptied, the box passes ⌘⏎ on: the pick is the answer.
      type(own(), '');
      type(box(), '');
      choice('Commit it').click();
      return wait(50);
    })
    .then(function () {
      box().focus();
      cmdEnter(box());
      return until(function () { return answers().length > 0; }, 3000).catch(function () {});
    })
    .then(function () {
      var a = answers()[0] || {};
      check('with the message box empty, ⌘⏎ answers with the pick', (a.answers || {})['How should I handle the evidence?'] === 'Commit it', JSON.stringify(a));
    })
    .then(done)
    .catch(stopped);
})();
