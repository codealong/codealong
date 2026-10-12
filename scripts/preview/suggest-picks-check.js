// Checks choices another session picked for the questions a session asks (`codealong session
// suggest --pick`): its question box shows them picked, each marked, with who suggested them and
// why, and no list of replies; Answer goes at once, with them. A pick of yours changes them, the
// Suggested chip stays on the suggested choice, and Dismiss takes away the ones of questions you didn't
// touch, words you typed counting as a touch. The row's Suggested chip says what was picked.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/suggest-picks-check.js
// As PREVIEW_INIT after `window.__picks_screenshot = true` it leaves the box as it opened, for a
// screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var failOnce = true;
  var suggestedAt = Date.now() - 30e3;
  var pending = {
    id: 'ask-9',
    label: 'Asks you · Which color do you prefer?',
    input: null,
    cannot: null,
    questions: [
      { question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Red', description: 'The color red' }, { label: 'Blue' }] },
      { question: 'Which days suit you?', options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed' }], multiSelect: true },
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
      // The first answer is turned down, so the box stays to go on with.
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
          s.suggestions = undefined;
          if (picked ? s.key !== picked.key : l.hidden || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = pending.label;
          s.suggestions = {
            replies: [],
            picks: { 'Which color do you prefer?': ['Blue'], 'Which days suit you?': ['Mon', 'Wed'] },
            note: 'Blue matches the rest of the app.',
            by: 'claude:claude:sup',
            byTitle: 'Supervisor',
            at: suggestedAt,
            promptAt: s.lastPromptAt,
          };
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
  var marked = function (b) { return !!b.querySelector('.ca-suggestedchip'); };
  var showing = function () { var l = box().querySelector('.ca-question .ca-approval-label'); return l ? l.textContent : ''; };
  var steps = function () { return Array.prototype.slice.call(box().querySelectorAll('.ca-question-steps button')); };
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('suggest picks check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      var chip = row().querySelector('.ca-suggestedchip');
      check('the row says it has picks', !!chip && /Suggested by Supervisor, picked to answer its questions with/.test(chip.title) && /Which days suit you\? Mon, Wed/.test(chip.title), chip ? chip.title : 'no chip');
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box() && button('Blue'); }, 5000);
    })
    .then(function () {
      check('the suggested choice is picked', on(button('Blue')) && !on(button('Red')) && /Which color/.test(showing()));
      check('and marked', marked(button('Blue')) && !marked(button('Red')));
      var desc = button('Red').querySelector('.ca-question-desc');
      check('a choice shows what it says about it', !!desc && desc.textContent === 'The color red' && !button('Blue').querySelector('.ca-question-desc'));
      var by = box().querySelector('.ca-suggest-head .ca-suggest-by');
      var note = box().querySelector('.ca-suggest-note');
      check('who suggested it and why', !!by && by.textContent === 'Suggested by Supervisor' && !!note && note.textContent === 'Blue matches the rest of the app.', by ? by.textContent : 'no head');
      check('no list of replies', !document.querySelector('.ca-sessinsp .ca-suggest-reply'));
      check('the strip marks both answered, without sparkles', steps().length === 2 && steps().every(function (b) { return !!b.querySelector('svg') && !b.querySelector('.ca-suggestedchip'); }));
      check('answer at once', !!button('Answer') && !button('Answer').disabled && !button('Next'));
      if (window.__picks_screenshot) return new Promise(function () {});
      button('Answer').click();
      return until(function () { return answers().length === 1; }, 3000);
    })
    .then(function () {
      var a = answers()[0][1].answers || {};
      check('answers with the picks', a['Which color do you prefer?'] === 'Blue' && JSON.stringify(a['Which days suit you?']) === '["Mon","Wed"]', JSON.stringify(a));
      return until(function () { return box() && !box().querySelector('.ca-composer-note').textContent.match(/Answering/); }, 3000);
    })
    .then(function () {
      steps()[1].click();
      return tick();
    })
    .then(function () {
      check('several picked where it takes more', /Which days/.test(showing()) && on(button('Mon')) && on(button('Wed')) && !on(button('Tue')), showing());
      button('Mon').click();
      return tick();
    })
    .then(function () {
      check('your pick changes them', !on(button('Mon')) && on(button('Wed')) && marked(button('Mon')));
      // Words of yours with them are an answer begun, which a change to the suggestions leaves be.
      type(box().querySelector('.ca-question-own'), 'Fri');
      return tick();
    })
    .then(function () {
      button('Dismiss').click();
      return tick();
    })
    .then(function () {
      check('dismiss asks the engine', window.__calls.some(function (c) { return c[0] === 'dismiss_suggestions' && (c[1] || {}).key === picked.key; }));
      check('and takes away the head and the marks', !box().querySelector('.ca-suggest-head') && !marked(button('Wed')));
      check('an answer you began keeps its picks', on(button('Wed')) && !on(button('Mon')));
      steps()[0].click();
      return tick();
    })
    .then(function () {
      check('the untouched one has none now', /Which color/.test(showing()) && !on(button('Blue')) && !on(button('Red')) && !marked(button('Blue')), showing());
      button('Red').click();
      return tick();
    })
    .then(function () {
      button('Answer').click();
      return until(function () { return answers().length === 2; }, 3000);
    })
    .then(function () {
      var a = answers()[1][1].answers || {};
      check('answers with yours', a['Which color do you prefer?'] === 'Red' && JSON.stringify(a['Which days suit you?']) === '["Wed","Fri"]', JSON.stringify(a));
    })
    .then(done)
    .catch(stopped);
})();
