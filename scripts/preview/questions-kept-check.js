// Checks that your answers to the questions a session asks are kept as you go (drafts.ts
// `AnswersDraft`, SessionReply.tsx `Questions`): picks, your own words and the question you were
// on are there again as you go to another session and back, show as another window writes them,
// start afresh for another call though it asks the same, and go once the call is answered.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-kept-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var other = null;
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
          s.raised = undefined;
          s.suggestions = undefined;
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
  var away = function () {
    choose(other);
    return until(function () { return !box(); }, 3000, 'another session').then(function () {
      choose(picked);
      return until(function () { return box() && q('.ca-question-choice', box()); }, 5000, 'the session again');
    });
  };

  say('questions kept check: waiting for the questions');
  until(function () { return rowOf(picked) && other; }, 20000, 'a session to ask and another')
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
      check('your answers are kept as you go', k.call === 'ask-21' && k.at === 1 && JSON.stringify(k.picks) === JSON.stringify({ 'Which color do you prefer?': ['Red'], 'Which days suit you?': ['Tue'] }) && k.own['Which days suit you?'] === 'mornings only', stored());
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
      // Another window (a peek on the same session) picks Blue and writes nothing of its own.
      var there = { call: 'ask-21', picks: { 'Which color do you prefer?': ['Blue'], 'Which days suit you?': ['Tue'] }, own: { 'Which color do you prefer?': '', 'Which days suit you?': 'mornings only' }, at: 0 };
      window.dispatchEvent(new StorageEvent('storage', { key: 'codealong.draft.answers:' + picked.key, newValue: JSON.stringify({ text: JSON.stringify(there), at: Date.now() }) }));
      return until(function () { return on(choice('Blue')); }, 2000, 'another window’s pick');
    })
    .then(function () {
      check('what another window picks shows here', on(choice('Blue')) && !on(choice('Red')));
      // Another call that asks the same, once this one's answered where it runs.
      pending = Object.assign({}, pending, { id: 'ask-22' });
      return away();
    })
    .then(function () {
      check('another call starts afresh, though it asks the same', /Which color/.test(showing()) && !on(choice('Red')) && !on(choice('Blue')) && !q('svg', steps()[0]), showing());
      check('and the last call’s answers go', stored() === null, stored());
      choice('Red').click();
      return until(function () { return /Which days/.test(showing()); }, 2000, 'the second question');
    })
    .then(function () {
      choice('Mon').click();
      return until(function () { return on(choice('Mon')); }, 2000, 'Mon picked');
    })
    .then(function () {
      q('.ca-composer-foot .ca-btn--primary', box()).click();
      return until(function () { return calls('session_answer').length > 0; }, 3000, 'the answer');
    })
    .then(function () {
      var a = sent('session_answer')[0] || {};
      check('Answer sends them', JSON.stringify(a.answers) === JSON.stringify({ 'Which color do you prefer?': 'Red', 'Which days suit you?': ['Mon'] }), JSON.stringify(a));
      return until(function () { return stored() === null; }, 3000, 'the kept answers gone');
    })
    .then(function () {
      check('answered, they go', stored() === null, stored());
    })
    .then(done)
    .catch(stopped);
})();
