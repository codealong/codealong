// Checks questions a session asks that Codealong can't answer (Claude Code's AskUserQuestion when
// the plugin's hook didn't hand them over): the reply area shows each question with its numbered
// choices and their descriptions, no Answer, and Open in the session's terminal; the thread names
// a question call by its headers and, opened, shows an earlier call's questions with your picks.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/questions-elsewhere-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=questions-elsewhere) it leaves the panel open, the answered
// call opened, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var earlier = [
    { question: 'Which record should the new check read?', header: 'INR-AAR', options: [
      { label: 'Lesson status (Recommended)', description: 'Each lesson line ends with `done: <link>` or `due: YYYY-MM-DD`; a repo check fails on one with neither.' },
      { label: 'Quarterly review lists lessons', description: 'The quarterly review must list every incident’s lessons. Lighter, but checks once a quarter.' },
      { label: 'Leave it for now', description: 'Keep it at 1 of 2 passing and revisit later.' },
    ] },
    { question: 'Which days should it run?', header: 'Days', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Wed' }, { label: 'Fri' }] },
  ];
  var waiting = [
    { question: 'The new restore method needs the newest timed restore test to be recent. Which limit?', header: 'RPL-TRC', options: [
      { label: '100 days (Recommended)', description: 'A quarter and a bit: the last test passes today with room to spare.' },
      { label: '45 days', description: 'Tighter; the last test is already 52 days old, so it fails until the next one.' },
    ] },
    { question: 'Should the monthly review be drafted today?', header: 'PIY-RIS', options: [
      { label: 'Draft it now (Recommended)', description: 'I write the first entry from this month’s KSI numbers for you to edit.' },
      { label: 'Leave it to you' },
    ] },
  ];
  var pending = { id: 'toolu_wait', label: 'Asks you · ' + waiting[0].question, input: null, cannot: 'Codealong didn’t get these as they were asked, so they’re answered where the session runs.', questions: waiting };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_pending' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'session_page' && mine) {
      window.__calls.push([cmd, args, options]);
      var at = Date.now() - 20 * 60e3;
      var item = function (n, kind, text, extra) { return Object.assign({ at: at + n * 60e3, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({ before: null, after: 7, items: [
        item(0, 'you', 'Add second methods to the KSIs that have one'),
        item(1, 'tool', 'AskUserQuestion', { id: 'toolu_done', input: '{ "questions": [] }', questions: earlier }),
        item(2, 'result', '', { id: 'toolu_done', output: 'Your questions have been answered.', answers: { 'Which record should the new check read?': ['Lesson status (Recommended)'], 'Which days should it run?': ['Mon', 'Fri'] } }),
        item(3, 'agent', 'Lesson status it is. Two more to settle before I write the checks.'),
        item(4, 'tool', 'AskUserQuestion', { id: 'toolu_wait', input: '{ "questions": [] }', questions: waiting }),
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || s.visitor || s.archived || s.harness !== 'claude' || s.surface !== 'cli') return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.stalledSince = null;
          s.asks = false;
          s.hosted = false;
          s.live = true;
          s.excerpt = pending.label;
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-elsewhere'); };
  var tools = function () { return document.querySelectorAll('.ca-sessinsp .ca-thread-tool'); };
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var text = function (el) { return el ? el.textContent.replace(/\s+/g, ' ').trim() : ''; };

  say('questions elsewhere check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return box() && tools().length === 2; }, 5000);
    })
    .then(function () {
      var b = box();
      check('both questions', b.querySelectorAll('.ca-question').length === 2);
      var first = b.querySelectorAll('.ca-question-list')[0];
      check('choices numbered', first && first.tagName === 'OL' && first.querySelectorAll('li').length === 2);
      check('descriptions shown', /A quarter and a bit/.test(text(first)));
      var buttons = Array.prototype.map.call(b.querySelectorAll('button'), text);
      check('no answering here', buttons.indexOf('Answer') < 0 && buttons.indexOf('Decline') < 0 && !b.querySelector('input'), buttons.join('|'));
      var open = Array.prototype.filter.call(b.querySelectorAll('button'), function (x) { return /^Open in /.test(text(x)); })[0];
      check('open where it runs', !!open, buttons.join('|'));
      check('says where', /^Answer them in /.test(text(b.querySelector('.ca-composer-note'))), text(b.querySelector('.ca-composer-note')));
      var labels = Array.prototype.map.call(tools(), function (t) { return text(t.querySelector('.ca-thread-toollabel')); });
      check('lines by their headers', labels[0] === 'Asks you · INR-AAR, Days' && labels[1] === 'Asks you · RPL-TRC, PIY-RIS', labels.join('|'));
      // The waiting call stays shut: the reply area below has its questions.
      check('waiting call shut', !tools()[1].querySelector('.ca-questions'));
      var before = window.__calls.length;
      if (!window.__questions_screenshot) open.click();
      tools()[0].querySelector('.ca-thread-toolhead').click();
      return tick().then(function () { return before; });
    })
    .then(function (before) {
      if (!window.__questions_screenshot) check('open asks', window.__calls.slice(before).some(function (c) { return /open/.test(c[0]); }), window.__calls.slice(before).map(function (c) { return c[0]; }).join('|'));
      var done = tools()[0];
      var lists = done.querySelectorAll('.ca-question-list');
      check('answered opens as questions', lists.length === 2 && !done.querySelector('pre'));
      var pickedText = Array.prototype.map.call(done.querySelectorAll('li.is-picked'), function (li) { return text(li.querySelector('.ca-question-opt')); });
      check('your picks marked', pickedText.join('|') === 'Lesson status (Recommended)|Mon|Fri', pickedText.join('|'));
      if (window.__questions_screenshot) done.scrollIntoView({ block: 'start' });
    })
    .then(done)
    .catch(stopped);
})();
