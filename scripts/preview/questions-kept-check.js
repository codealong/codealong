// Checks that your answers to the questions a session asks are kept as you go (drafts.ts
// `AnswersDraft`, SessionReply.tsx `Questions`): picks, your own words and the question you were
// on are there again as you go to another session and back, show as another window writes them
// (and empties them), start afresh for another call though it asks the same and for the call's
// questions offering other choices, are taken over by newer picks after an answer that didn't go,
// and go once the call is answered. That a question keeps its id through a server restart is
// claude/asks.rs's (`request_id`).
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-kept-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var pending = {
    id: 'ask-21',
    label: 'Asks you · Which color do you prefer?',
    input: null,
    cannot: null,
    questions: [
      { question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Red', description: 'The color red' }, { label: 'Blue' }] },
      { question: 'Which days suit you?', header: 'Days', options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed' }], multiSelect: true },
    ],
  };
  var failOnce = true;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      // The first answer is turned down, so the box comes back with what it sent.
      if (failOnce) {
        failOnce = false;
        return Promise.reject('It isn’t waiting on that any more.');
      }
      return Promise.resolve('Answered.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.suggestions = undefined;
          if (l.hidden || s.visitor || s.archived) return;
          if (picked && picked.key !== s.key) {
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
  // Another session's row: any the list shows, since the first the board has may be filtered out.
  var otherRow = function () { return all('[id^="row-"]').filter(function (r) { return r.id.slice(r.id.indexOf('::') + 2) !== picked.key; })[0]; };
  var press = function (row) {
    row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    row.click();
  };
  var choose = function (it) { press(rowOf(it)); };
  var box = function () { return q('.ca-composer.is-questions'); };
  var choice = function (text) {
    return all('.ca-question-choice', box()).filter(function (b) { return q('.ca-question-choice-label > span', b).textContent.trim() === text; })[0];
  };
  var on = function (b) { return !!b && b.getAttribute('aria-checked') === 'true'; };
  var own = function () { return q('.ca-question-own', box()); };
  var showing = function () { var l = q('.ca-question .ca-approval-label', box()); return l ? l.textContent : ''; };
  var steps = function () { return all('.ca-question-steps button', box()); };
  var stored = function () { return localStorage.getItem('codealong.draft.answers:' + picked.key); };
  // What's kept, as the box reads it: a draft whose text is the answers' JSON.
  var kept = function () { var raw = stored(); return raw && JSON.parse(JSON.parse(raw).text); };
  // Another window (a peek on the same session) keeps answers, or empties them: storage events
  // reach only the other windows, so this one is told as they'd tell it.
  var fromThere = function (answers) {
    var name = 'codealong.draft.answers:' + picked.key;
    var value = answers ? JSON.stringify({ text: JSON.stringify(answers), at: Date.now() }) : null;
    if (value) localStorage.setItem(name, value);
    else localStorage.removeItem(name);
    window.dispatchEvent(new StorageEvent('storage', { key: name, newValue: value }));
  };
  // The questions as answers are kept for (drafts.ts `askedAs`).
  var asked = function () { return JSON.stringify(pending.questions.map(function (x) { return [x.question, !!x.multiSelect, x.options.map(function (o) { return o.label; })]; })); };
  var answerButton = function () { return q('.ca-composer-foot .ca-btn--primary', box()); };
  var away = function () {
    press(otherRow());
    return until(function () { return !box(); }, 3000, 'another session').then(function () {
      choose(picked);
      return until(function () { return box() && q('.ca-question-choice', box()); }, 5000, 'the session again');
    });
  };

  say('questions kept check: waiting for the questions');
  until(function () { return rowOf(picked) && otherRow(); }, 20000, 'a session to ask and another')
    .then(function () {
      choose(picked);
      return until(function () { return box() && choice('Red'); }, 5000, 'the questions');
    })
    .then(function () {
      check('nothing kept before you pick', stored() === null, stored());
      choice('Red').click();
      return until(function () { return /Which days/.test(showing()); }, 2000, 'the second question');
    })
    .then(function () {
      choice('Tue').click();
      type(own(), 'mornings only');
      var k = kept() || {};
      check('your answers are kept as you go', k.call === pending.id && k.asked === asked() && k.at === 1 && JSON.stringify(k.picks) === JSON.stringify({ 'Which color do you prefer?': ['Red'], 'Which days suit you?': ['Tue'] }) && k.own['Which days suit you?'] === 'mornings only', stored());
      return away();
    })
    .then(function () {
      check('back again, on the question you were on', /Which days/.test(showing()), showing());
      check('with your picks', on(choice('Tue')) && !on(choice('Mon')) && !on(choice('Wed')));
      check('and your words', own().value === 'mornings only', own().value);
      check('the first question still answered', !!q('svg', steps()[0]));
      steps()[0].click();
      return until(function () { return /Which color/.test(showing()); }, 2000, 'the first question');
    })
    .then(function () {
      check('its pick is kept too', on(choice('Red')) && !on(choice('Blue')));
      // Another window picks Blue.
      fromThere({ call: pending.id, asked: asked(), picks: { 'Which color do you prefer?': ['Blue'], 'Which days suit you?': ['Tue'] }, own: { 'Which color do you prefer?': '', 'Which days suit you?': 'mornings only' }, at: 0 });
      return until(function () { return on(choice('Blue')); }, 2000, 'another window’s pick');
    })
    .then(function () {
      check('what another window picks shows here', on(choice('Blue')) && !on(choice('Red')));
      check('and isn’t written back', kept().picks['Which color do you prefer?'][0] === 'Blue', stored());
      // Another window takes everything back, on the first question.
      fromThere(null);
      return until(function () { return !on(choice('Blue')); }, 2000, 'another window emptying them');
    })
    .then(function () {
      check('what another window empties empties here', !on(choice('Red')) && !q('svg', steps()[1]));
      check('and nothing is written back', stored() === null, stored());
      // Something picked, then another call asking the same, once this one's answered where it runs.
      choice('Blue').click();
      return until(function () { return !!stored(); }, 2000, 'Blue kept');
    })
    .then(function () {
      pending = Object.assign({}, pending, { id: 'ask-22' });
      return away();
    })
    .then(function () {
      check('another call starts afresh, though it asks the same', /Which color/.test(showing()) && !on(choice('Red')) && !on(choice('Blue')) && !q('svg', steps()[0]), showing());
      check('and the last call’s answers go', stored() === null, stored());
      choice('Blue').click();
      return until(function () { return !!stored(); }, 2000, 'Blue kept');
    })
    .then(function () {
      // The call's questions offering other choices: Blue isn't one to send.
      pending = Object.assign({}, pending, { questions: [{ question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Green' }, { label: 'Yellow' }] }, pending.questions[1]] });
      return away();
    })
    .then(function () {
      check('other choices start afresh', /Which color/.test(showing()) && !!choice('Green') && !on(choice('Green')) && !on(choice('Yellow')) && !q('svg', steps()[0]), showing());
      check('and nothing is ready to answer', answerButton().textContent.trim().indexOf('Next') === 0 || answerButton().disabled, answerButton().textContent);
      check('and the answers to the others go', stored() === null, stored());
      // Another call.
      pending = Object.assign({}, pending, { id: 'ask-23', questions: [{ question: 'Which shade do you prefer?', header: 'Shade', options: [{ label: 'Red' }, { label: 'Blue' }] }, pending.questions[1]] });
      return away();
    })
    .then(function () {
      check('the next call’s questions show', /Which shade/.test(showing()) && !on(choice('Red')), showing());
      // An answer that doesn't go, then a change: the change is what's kept.
      choice('Red').click();
      return until(function () { return /Which days/.test(showing()); }, 2000, 'the second question');
    })
    .then(function () {
      choice('Mon').click();
      return until(function () { return on(choice('Mon')); }, 2000, 'Mon picked');
    })
    .then(function () {
      answerButton().click();
      // The box goes as it answers, and comes back saying why it didn't go.
      return until(function () { var note = box() && q('.ca-question .ca-question-choice', box()) && q('.ca-composer-note', box()); return calls('session_answer').length === 1 && note && /any more/.test(note.textContent); }, 5000, 'the answer turned down');
    })
    .then(function () {
      check('an answer that didn’t go keeps them', /Which days/.test(showing()) && on(choice('Mon')) && !!stored(), showing());
      choice('Wed').click();
      return until(function () { return on(choice('Wed')); }, 2000, 'Wed picked');
    })
    .then(function () { return away(); })
    .then(function () {
      check('a change after it is kept over it', /Which days/.test(showing()) && on(choice('Mon')) && on(choice('Wed')), showing());
      answerButton().click();
      return until(function () { return calls('session_answer').length === 2; }, 3000, 'the answer');
    })
    .then(function () {
      var a = sent('session_answer')[1] || {};
      check('Answer sends them', JSON.stringify(a.answers) === JSON.stringify({ 'Which shade do you prefer?': 'Red', 'Which days suit you?': ['Mon', 'Wed'] }), JSON.stringify(a));
      return until(function () { return stored() === null; }, 3000, 'the kept answers gone');
    })
    .then(function () {
      check('answered, they go', stored() === null, stored());
    })
    .then(done)
    .catch(stopped);
})();
