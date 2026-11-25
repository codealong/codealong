// Checks the panel answering a form a session Codealong runs is asked to fill in (acp/forms.rs):
// what the form says it wants above its questions, a field that takes only its choices with no box
// for words of your own, a box that says what it takes (an email address, a whole number), a
// secret hidden as it's typed, Answer held back while a field the form needs has no answer and
// saying which, Decline saying its agent goes on, and Answer sending every answer keyed by its
// question.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/forms-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=forms, and `window.__questions_screenshot = true` before it)
// it leaves the panel open on the email, the region picked, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var says = 'The deploy server needs a few details before it starts.';
  var pending = {
    id: 'form-7',
    label: 'Asks you · ' + says,
    input: says,
    cannot: null,
    questions: [
      { question: 'Which region should it deploy to?', header: 'Region', options: [{ label: 'United States' }, { label: 'Europe', description: 'Frankfurt and Dublin' }], form: { choicesOnly: true, required: true } },
      { question: 'Where should it send the report?', header: 'Email', options: [], form: { required: true, hint: 'An email address' } },
      { question: 'How many replicas should it run?', header: 'Replicas', options: [], form: { hint: 'A whole number from 1 to 5' } },
      { question: 'Paste the deploy token', header: 'Token', options: [], form: { secret: true } },
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
          if (picked || l.hidden || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = true;
          s.excerpt = pending.label;
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
  var own = function () { return box().querySelector('.ca-question-own'); };
  var step = function (text) { return Array.prototype.filter.call(box().querySelectorAll('.ca-question-steps button'), function (b) { return b.textContent.trim() === text; })[0]; };
  var showing = function () { var l = box().querySelector('.ca-question .ca-approval-label'); return l ? l.textContent : ''; };
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('forms check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box(); }, 5000);
    })
    .then(function () {
      var plan = box().querySelector('.ca-approval-plan');
      check('what it says it wants, above its questions', !!plan && plan.textContent.indexOf(says) >= 0 && !!(plan.compareDocumentPosition(box().querySelector('.ca-question')) & Node.DOCUMENT_POSITION_FOLLOWING), plan ? plan.textContent : 'none');
      check('a strip of its fields', Array.prototype.map.call(box().querySelectorAll('.ca-question-steps button'), function (b) { return b.textContent.trim(); }).join('|') === 'Region|Email|Replicas|Token');
      check('only its choices for the region', !!button('Europe') && !own(), showing());
      check('decline says its agent goes on', /its agent is told and goes on/.test(button('Decline').title), button('Decline').title);
      button('Europe').click();
      return tick();
    })
    .then(function () {
      check('on to the email, its box saying what it takes', /send the report/.test(showing()) && !!own() && own().placeholder === 'An email address' && own().type === 'text', showing() + ' · ' + (own() && own().placeholder));
      if (window.__questions_screenshot) return;
      step('Replicas').click();
      return tick()
        .then(function () {
          check('a whole number, and between what', own().placeholder === 'A whole number from 1 to 5', own().placeholder);
          type(own(), '3');
          step('Token').click();
          return tick();
        })
        .then(function () {
          check('a secret hidden as it is typed', own().type === 'password', own().type);
          type(own(), 'abc');
          step('Email').click();
          return tick();
        })
        .then(function () {
          check('answer held back while the email has none, saying so', !!button('Answer') && button('Answer').disabled && button('Answer').title === 'Answer “Email” first: the form needs it', button('Answer') && button('Answer').title);
          type(own(), 'me@example.com');
          return tick();
        })
        .then(function () {
          check('answer ready', !button('Answer').disabled);
          button('Answer').click();
          return until(function () { return answers().length > 0; }, 3000);
        })
        .then(function () {
          var c = answers()[0][1];
          var a = c.answers || {};
          check('sends every answer by its question', c.id === 'form-7' && a['Which region should it deploy to?'] === 'Europe' && a['Where should it send the report?'] === 'me@example.com' && a['How many replicas should it run?'] === '3' && a['Paste the deploy token'] === 'abc', JSON.stringify(c));
        });
    })
    .then(done)
    .catch(stopped);
})();
