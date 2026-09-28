// Checks the panel answering questions a session asks (Claude Code's AskUserQuestion, held
// through the plugin's hook): each question shows its choices and a box for your own answer,
// Answer waits for a pick, a question that takes one keeps one, words typed replace its pick,
// and Answer sends the picks, keyed by question.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions) it leaves the panel open, with picks, for a
// screenshot.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };

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
  var own = function (i) { return box().querySelectorAll('.ca-question-own')[i]; };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var on = function (b) { return b.getAttribute('aria-checked') === 'true'; };

  say('questions check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box(); }, 5000);
    })
    .then(function () {
      check('two questions', box().querySelectorAll('.ca-question').length === 2);
      check('waits for a pick', button('Answer').disabled);
      button('Red').click();
      return tick();
    })
    .then(function () {
      button('Blue').click();
      button('Mon').click();
      button('Wed').click();
      return tick();
    })
    .then(function () {
      check('one pick where it takes one', !on(button('Red')) && on(button('Blue')));
      check('several where it takes more', on(button('Mon')) && on(button('Wed')) && !on(button('Tue')));
      check('answer ready', !button('Answer').disabled);
      if (window.__questions_screenshot) return;
      type(own(0), 'Green');
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
    .then(function () { say('done: ' + out.join(', ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(', ')); });
})();
