// Checks minimizing the questions a session asks (SessionReply.tsx `AsksHead`): the button at the
// box's head leaves the head alone, saying the question showing on one line and how many more
// there are, and gives the room to the conversation above; a click on the question (or ⌘⏎, where
// ⌘⏎ answers) shows the box again with your picks and the question you were on, and answers
// nothing. In the main window the box stays minimized as you go to another session and back, and
// the next call it waits on shows whole though it asks the same. ⌘⏎ is Ctrl+Enter on Linux.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-minimize-check.js
//   PREVIEW_WAIT=30000 scripts/preview.sh waiting '' scripts/preview/questions-minimize-check.js
//   PREVIEW_WAIT=30000 PREVIEW_PLATFORM=linux scripts/preview.sh waiting '' scripts/preview/questions-minimize-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions-minimized) it leaves the box minimized, for a
// screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  var picked = null;
  var other = null;
  var pending = {
    id: 'ask-12',
    label: 'Asks you · Which color do you prefer?',
    input: null,
    cannot: null,
    questions: [
      { question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Red', description: 'The color red' }, { label: 'Blue' }] },
      { question: 'Which days suit you for the review, given the release goes out on Thursday and the notes need a day?', header: 'Days', options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed' }], multiSelect: true },
    ],
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
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
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || s.visitor || s.archived) return;
          if (picked && picked.key !== s.key) {
            if (!other) other = { key: s.key, lane: l.id };
            return;
          }
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = pending.label;
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var rowOf = function (it) { return it && document.getElementById('row-' + it.lane + '::' + it.key); };
  var choose = function (it) {
    rowOf(it).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    rowOf(it).click();
  };
  var box = function () { return q('.ca-composer.is-questions'); };
  var fold = function () { return q('.ca-approval-fold', box()); };
  var folded = function () { return q('.ca-approval-folded', box()); };
  var thread = function () { return q('.ca-thread', box().parentElement); };
  var choice = function (text) {
    return all('.ca-question-choice', box()).filter(function (b) { return q('.ca-question-choice-label > span', b).textContent.trim() === text; })[0];
  };
  var on = function (b) { return !!b && b.getAttribute('aria-checked') === 'true'; };
  var showing = function () { var l = q('.ca-question .ca-approval-label', box()); return l ? l.textContent : ''; };
  var answered = function () { return calls('session_answer').length > 0; };
  // The main window lists the sessions; the waiting window shows the one that waits.
  var listed = false;
  var before = 0;

  say('questions minimize check: waiting for the questions');
  until(function () { return rowOf(picked) || (box() && q('.ca-question-choice', box())); }, 20000)
    .then(function () {
      listed = !!rowOf(picked);
      if (listed) choose(picked);
      return until(function () { return box() && choice('Red'); }, 5000, 'the questions');
    })
    .then(function () {
      choice('Red').click();
      return until(function () { return /Which days/.test(showing()); }, 2000, 'the second question');
    })
    .then(function () {
      choice('Tue').click();
      before = thread().clientHeight;
      check('a button minimizes it', !!fold() && fold().getAttribute('aria-expanded') === 'true' && fold().getAttribute('aria-label') === 'Minimize');
      fold().click();
      return until(function () { return folded(); }, 2000, 'the box minimized');
    })
    .then(function () {
      check('only its head shows', !q('.ca-question-choice', box()) && !q('.ca-question-own', box()) && !q('.ca-composer-foot', box()) && !q('.ca-question-steps', box()));
      check('the head says the question showing', /Which days suit you/.test(folded().textContent) && folded().title === pending.questions[1].question, folded().textContent);
      check('and how many more', q('.ca-approval-folded-more', box()) && q('.ca-approval-folded-more', box()).textContent === '+1 more');
      var text = q('.ca-approval-folded-text', box());
      check('on one line', text.scrollHeight <= text.clientHeight + 1 && box().offsetHeight < 60, 'box ' + box().offsetHeight + 'px');
      check('the conversation takes the room', thread().clientHeight > before + 100, before + ' → ' + thread().clientHeight);
      check('the button shows it again', fold().getAttribute('aria-expanded') === 'false' && fold().getAttribute('aria-label') === 'Show the questions');
      if (window.__questions_minimized) return;
      // Where ⌘⏎ answers (the waiting window), minimized it shows the box again instead.
      if (listed) folded().click();
      else key(document.body, { key: 'Enter', code: 'Enter', metaKey: !linux, ctrlKey: linux });
      return until(function () { return !folded() && q('.ca-question-choice', box()); }, 2000, 'the box shown again').then(function () {
        check(listed ? 'a click on the question shows it again' : '⌘⏎ shows it again', true);
        check('on the question it was on', /Which days/.test(showing()), showing());
        check('with your picks', on(choice('Tue')) && !on(choice('Mon')) && !!q('.ca-question-steps button svg', box()));
        check('and answers nothing', !answered());
        if (!listed || !other) return;
        // Minimized, it stays so as you go to another session and back.
        fold().click();
        return until(function () { return folded(); }, 2000, 'minimized again')
          .then(function () {
            choose(other);
            return until(function () { return !box(); }, 3000, 'another session');
          })
          .then(function () {
            choose(picked);
            return until(function () { return box(); }, 5000, 'the session again');
          })
          .then(function () {
            check('back again, it stays minimized', !!folded() && !q('.ca-question-choice', box()));
            // Another call that asks the same, once this one's answered where it runs.
            pending = Object.assign({}, pending, { id: 'ask-13' });
            choose(other);
            return until(function () { return !box(); }, 3000, 'another session');
          })
          .then(function () {
            choose(picked);
            return until(function () { return box() && q('.ca-question-choice', box()); }, 5000, 'the next call');
          })
          .then(function () {
            check('the next call shows whole, though it asks the same', !folded());
          });
      });
    })
    .then(done)
    .catch(stopped);
})();
