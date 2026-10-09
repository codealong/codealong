// Checks the panel answering questions a session asks (Claude Code's AskUserQuestion, held
// through the plugin's hook): several show one at a time, under a strip of their headers that
// ticks each once answered, each with its choices and a box for your own answer. Next goes on
// while another has no answer, a pick on a question that takes one goes on by itself, the strip
// goes back, a question that takes one keeps one, words typed replace its pick, and Answer sends
// the picks, keyed by question. What the session wrote with them isn't in its record yet (Claude
// Code writes it once they're answered), and the box says where it is.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions) it leaves the panel open, with picks, for a
// screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var pending = {
    id: 'ask-7',
    label: 'Asks you · Which color do you prefer?',
    input: null,
    cannot: null,
    questions: [
      { question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Red', description: 'The color red' }, { label: 'Blue' }] },
      { question: 'Which days suit you?', options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed' }], multiSelect: true },
    ],
    unwritten: true,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Answered: Which color do you prefer? → Green; Which days suit you? → Mon, Wed.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || s.visitor || s.archived) return;
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

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-questions'); };
  var button = function (text) {
    return Array.prototype.filter.call(box().querySelectorAll('button'), function (b) { return b.textContent.trim() === text; })[0];
  };
  var own = function () { return box().querySelector('.ca-question-own'); };
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var on = function (b) { return b.getAttribute('aria-checked') === 'true'; };
  var showing = function () { var l = box().querySelector('.ca-question .ca-approval-label'); return l ? l.textContent : ''; };
  var steps = function () { return Array.prototype.slice.call(box().querySelectorAll('.ca-question-steps button')); };
  var ticked = function (i) { return !!steps()[i].querySelector('svg'); };

  say('questions check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box(); }, 5000);
    })
    .then(function () {
      check('one question at a time', box().querySelectorAll('.ca-question').length === 1 && /Which color/.test(showing()), showing());
      check('a strip of their headers', steps().map(function (b) { return b.textContent.trim(); }).join('|') === 'Color|Question 2', steps().map(function (b) { return b.textContent.trim(); }).join('|'));
      check('the header shows once', !box().querySelector('.ca-question .ca-question-header'));
      var said = box().querySelector('.ca-approval-unwritten');
      var link = box().querySelector('.ca-approval-head .ca-linkbtn');
      var where = link ? link.textContent.replace(/^Open in /, '').trim() : '';
      check('says where what it wrote is', !!said && /wrote with this question shows in the conversation once it’s answered; /.test(said.textContent) && !!where && said.textContent.indexOf(where + ' has it now.') > 0, said ? said.textContent : 'no line');
      check('next, while another has no answer', !!button('Next') && !button('Answer'));
      // Enter in your own answer's box goes on to the next question's box.
      own().focus();
      type(own(), 'Teal');
      key(own(), { key: 'Enter', code: 'Enter' });
      return tick();
    })
    .then(function () {
      check('enter goes on to the next box', /Which days/.test(showing()) && document.activeElement === own() && ticked(0), showing() + ' · focus on ' + (document.activeElement && document.activeElement.className) + ' · ticked ' + ticked(0));
      check('and answers nothing', !window.__calls.some(function (c) { return c[0] === 'session_answer'; }));
      steps()[0].click();
      return tick();
    })
    .then(function () {
      button('Red').click();
      return tick();
    })
    .then(function () {
      check('a pick on one that takes one goes on', /Which days/.test(showing()) && ticked(0) && !ticked(1), showing());
      check('answer once the others have one', !!button('Answer') && !button('Next'));
      button('Mon').click();
      button('Wed').click();
      return tick();
    })
    .then(function () {
      check('several where it takes more, and it stays', on(button('Mon')) && on(button('Wed')) && !on(button('Tue')) && /Which days/.test(showing()));
      steps()[0].click();
      return tick();
    })
    .then(function () {
      check('the strip goes back, with the pick', /Which color/.test(showing()) && on(button('Red')), showing());
      button('Blue').click();
      return tick();
    })
    .then(function () {
      check('one pick where it takes one', !on(button('Red')) && on(button('Blue')) && /Which color/.test(showing()));
      check('answer ready', !button('Answer').disabled);
      if (window.__questions_screenshot) return;
      type(own(), 'Green');
      return tick().then(function () {
        check('typing replaces the pick', !on(button('Blue')));
        button('Answer').click();
        return until(function () { return window.__calls.some(function (c) { return c[0] === 'session_answer'; }); }, 3000);
      }).then(function () {
        var c = window.__calls.filter(function (c) { return c[0] === 'session_answer'; })[0][1];
        var a = c.answers || {};
        check('sends the picks', c.id === 'ask-7' && a['Which color do you prefer?'] === 'Green' && JSON.stringify(a['Which days suit you?']) === '["Mon","Wed"]', JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
