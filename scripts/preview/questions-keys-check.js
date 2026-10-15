// Checks the message box in the waiting window while a session asks you questions (SessionReply.tsx
// `Questions`): words written there decline the questions and tell the session them instead, with
// ⏎ and with ⌘⏎, so you can say something other than an answer; words left half typed in a
// question's own box aren't sent as its answer, and a decline that doesn't go puts the words back
// in the box. With the box empty, ⌘⏎ answers with your pick, as from anywhere in the window.
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
      // A decline with words doesn't go, so the questions stay up for the pick after it. It takes
      // a moment, as a real one does, so the box is seen empty before the words come back.
      if (args.message) return wait(300).then(function () { throw 'It stopped waiting before the answer reached it.'; });
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

  var words = 'Can you explain what these records are in more detail?';
  var own = function () { return q('.ca-question-own'); };
  var box = function () { return q('.ca-composer:not(.is-approval) textarea'); };
  var send = function () { return q('.ca-composer:not(.is-approval) .ca-composer-send'); };
  // What the box says, on the strip at the top of the window.
  var boxNote = function () { var n = q('.ca-reply-queue-note'); return n ? n.textContent : ''; };
  var questionsNote = function () { var n = q('.ca-composer.is-questions .ca-composer-note'); return n ? n.textContent : ''; };
  var answers = function () { return sent('session_answer'); };
  var cmdEnter = function (el) { key(el, { key: 'Enter', code: 'Enter', metaKey: true }); };
  var enter = function (el) { key(el, { key: 'Enter', code: 'Enter' }); };
  var choice = function (text) {
    return all('.ca-question-choices button').filter(function (b) { return b.querySelector('.ca-question-choice-label > span').textContent.trim() === text; })[0];
  };
  var declined = function (a) { return !!a && a.answer === 'deny' && a.message === words && !a.answers; };
  var hint = function () { return box() ? box().getAttribute('aria-placeholder') || '' : ''; };
  // A decline is over once the questions say how it went: the stub turns it down, so the words are
  // back in the box. A busy Mac takes seconds over it, even stubbed.
  var back = function (n) {
    var gone = false;
    return function () {
      // It has to have gone from the box first: before that, the words are still there from the last one.
      gone = gone || box().value === '';
      return gone && answers().length >= n && /stopped waiting/.test(questionsNote()) && box().value === words;
    };
  };

  say('questions keys check: waiting for the question');
  until(function () { return own() && box() && questionsNote(); }, 20000)
    .then(function () {
      // The box hears what it does to them once the questions are read.
      return until(function () { return /^Or decline it/.test(hint()); }, 5000).catch(function () {});
    })
    .then(function () {
      check('the questions say the box below declines them', /write below to say something else/.test(questionsNote()), questionsNote());
      check('the empty box says it declines them', /^Or decline it/.test(hint()), hint());
      // Words left in the question's own box, then a message written below.
      type(own(), 'Can you expl');
      box().focus();
      type(box(), words);
      return wait(50);
    })
    .then(function () {
      check('with words in the box, it says ⏎ declines with them', /^⏎ declines it and sends this instead/.test(boxNote()), boxNote());
      check('its send button can be pressed', !!send() && !send().disabled && /^Decline it/.test(send().title), send() && send().title);
      enter(box());
      return until(back(1), 15000).catch(function () {});
    })
    .then(function () {
      check('⏎ declines with the words, not the half-typed answer', declined(answers()[0]), JSON.stringify(answers()));
      check('a decline that didn’t go has the words back in the box', box().value === words, box().value);
      cmdEnter(box());
      return until(back(2), 15000).catch(function () {});
    })
    .then(function () {
      check('⌘⏎ from the box with words declines with them too', declined(answers()[1]), JSON.stringify(answers()));
      // Emptied, the box passes ⌘⏎ on: the pick is the answer.
      type(own(), '');
      type(box(), '');
      choice('Commit it').click();
      return wait(50);
    })
    .then(function () {
      box().focus();
      cmdEnter(box());
      return until(function () { return answers().length > 2; }, 15000).catch(function () {});
    })
    .then(function () {
      var a = answers()[2] || {};
      check('with the message box empty, ⌘⏎ answers with the pick', a.answer === 'allow' && (a.answers || {})['How should I handle the evidence?'] === 'Commit it', JSON.stringify(answers()));
    })
    .then(done)
    .catch(stopped);
})();
