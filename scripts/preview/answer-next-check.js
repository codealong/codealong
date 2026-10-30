// Checks that answering a session's questions with ⌘⏎ goes on to the next that waits on you, as
// ⌘⏎ in the box does for a message or a call to allow (SessionReply.tsx `Questions`): two
// sessions that ask you something and one whose turn ended. ⌘⏎ in the box for your own answer
// sends it and goes on, the strip saying how it went; the Answer button answers and stays.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/answer-next-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 3600e3 },
    { status: 'unread', since: now - 5 * 60e3 },
  ];
  var pending = {
    id: 'ask-7',
    label: 'Asks you · Which color do you prefer?',
    input: null,
    cannot: null,
    questions: [{ question: 'Which color do you prefer?', header: 'Color', options: [{ label: 'Red' }, { label: 'Blue' }] }],
  };
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked.slice(0, 2).some(function (p) { return p.key === (args || {}).key; })) {
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
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Queue ' + 'ABC'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var asks = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-questions'); };
  var own = function () { return asks().querySelector('.ca-question-own'); };
  var button = function (text) { return Array.prototype.filter.call(asks().querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0]; };
  var answers = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }).map(function (c) { return c[1]; }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('answer next check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return until(asks, 5000); });
    })
    .then(function () {
      own().focus();
      type(own(), 'Green');
      return wait(50);
    })
    .then(function () {
      own().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
      return on('Queue B');
    })
    .then(function () {
      var a = answers();
      check('⌘⏎ on an answer you typed sends it', a.length === 1 && a[0].key === picked[0].key && a[0].answers['Which color do you prefer?'] === 'Green', JSON.stringify(a));
      check('and goes on to the next that waits on you', title() === 'Queue B', title());
      check('saying how it went', /Queue A: Answered\./.test(strip()), strip());
      return until(asks, 5000);
    })
    .then(function () {
      button('Red').click();
      return wait(50);
    })
    .then(function () {
      button('Answer').click();
      return until(function () { return answers().length === 2; }, 3000).then(function () { return wait(400); });
    })
    .then(function () {
      var a = answers();
      check('the Answer button answers and stays', a.length === 2 && a[1].key === picked[1].key && a[1].answers['Which color do you prefer?'] === 'Red' && title() === 'Queue B', title() + ' ' + JSON.stringify(a[1]));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
