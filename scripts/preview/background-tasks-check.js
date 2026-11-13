// Checks what a session's agent left running in the background (Session.tasks, BackgroundBits.tsx)
// in its header in the Sessions view: a chip among its links says how many run, a click lists
// them under the links with where each stands, Output shows the last of what one printed with a
// link to the address a server printed, and Stop asks the engine to stop it and says how it went.
// A session with nothing running and one ended says so instead. Put on real sessions of this Mac,
// since few have something running when the check runs.
//   scripts/preview.sh sessions '' scripts/preview/background-tasks-check.js
// It ends with the list open and the dev server's output shown, for a screenshot (light and dark):
//   PREVIEW_INIT=scripts/preview/background-tasks-check.js PREVIEW_NAME=background-tasks scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var min = 60e3;
  var plan = [
    {
      title: 'Background A',
      tasks: [
        { id: 'bfoz923q7', command: 'npm run dev -- --port 4321', about: 'Start the blog’s dev server', startedAt: now - 12 * min, output: '/private/tmp/claude-501/-Users-me-projects-northwind-app/s1/tasks/bfoz923q7.output' },
        { id: 'b70i34vi3', command: 'tail -f server.log | grep --line-buffered ERROR', about: 'errors in the server’s log', monitor: true, startedAt: now - 9 * min },
        { id: 'bsabc908a', command: 'cargo test -p atlas', about: 'Run the engine’s tests', startedAt: now - 8 * min, endedAt: now - 3 * min, status: 'failed', exitCode: 101 },
      ],
    },
    { title: 'Background B', tasks: [{ id: '19957', command: 'npm ci', startedAt: now - 7 * min, endedAt: now - 2 * min, status: 'completed', exitCode: 0 }] },
  ];
  var picked = [];
  var stopped = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_task_output') {
      window.__calls.push([cmd, args, options]);
      // As the engine gives it: as a terminal last showed it, its colors taken out (util.rs `terminal_text`).
      return Promise.resolve('  VITE v5.4.0  ready in 312 ms\n\n  ➜  Local:   http://localhost:4321/\n  ➜  Network: use --host to expose\n');
    }
    if (cmd === 'session_task_stop') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (res) { stopped = res; });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      var pinned = board.pinned || [];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          var i = picked.findIndex(function (x) { return x.key === s.key && x.lane === l.id; });
          if (i < 0 && (l.hidden || l.device || s.visitor || s.archived || s.helper || s.headless || s.harness !== 'claude' || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length)) return;
          seen[s.key] = true;
          if (i < 0) {
            i = picked.length;
            picked.push({ key: s.key, lane: l.id });
          }
          s.title = plan[i].title;
          s.status = 'working';
          s.statusSince = now - 12 * min;
          s.live = true;
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.tasks = plan[i].tasks;
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chip = function () { return document.querySelector('.ca-sessinsp-head .ca-taskchip'); };
  var rows = function () { return all('.ca-sessinsp-head .ca-task'); };
  var command = function (r) { return r.querySelector('.ca-task-command').textContent; };
  var state = function (r) { return r.querySelector('.ca-task-state').textContent; };
  var buttonIn = function (r, words) { return all('button', r).filter(function (b) { return b.textContent === words; })[0]; };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title && chip(); }, 3000, plan[i].title + ' shown with its chip').then(function () { return wait(50); });
  };

  say('Background tasks check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(1); }, 20000, 'two unpinned Claude Code sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () { return show(1); })
    .then(function () {
      check('one with only an ended command says that, neutral', chip().textContent === '1 ended' && chip().classList.contains('is-ended'), chip().textContent);
      return show(0);
    })
    .then(function () {
      check('the chip says how many run', chip().textContent === '2 running' && chip().classList.contains('is-running'), chip().textContent);
      check('and names them on hover', /npm run dev -- --port 4321 · Running · 12m/.test(chip().title) && /cargo test -p atlas · Failed · exit 101 · 3m ago/.test(chip().title), chip().title);
      check('nothing is listed until it’s clicked', rows().length === 0);
      chip().click();
      return until(function () { return rows().length === 3; }, 2000, 'the list under the links');
    })
    .then(function () {
      var r = rows();
      check('each says its command, running ones first', command(r[0]) === 'npm run dev -- --port 4321' && command(r[1]).indexOf('tail -f server.log') === 0 && command(r[2]) === 'cargo test -p atlas', r.map(command).join(' | '));
      check('and where it stands', state(r[0]) === 'Running · 12m' && state(r[2]) === 'Failed · exit 101 · 3m ago', r.map(state).join(' | '));
      check('what it’s for, a Monitor said as one', /Start the blog’s dev server/.test(r[0].textContent) && /Monitor: each line it prints goes to the agent · errors in the server’s log/.test(r[1].textContent), r[1].querySelector('.ca-task-about') && r[1].querySelector('.ca-task-about').textContent);
      check('running ones can be stopped, an ended one can’t', !!buttonIn(r[0], 'Stop') && !!buttonIn(r[1], 'Stop') && !buttonIn(r[2], 'Stop'));
      buttonIn(r[1], 'Stop').click();
      return until(function () { return calls('session_task_stop').length === 1; }, 2000, 'Stop asking the engine');
    })
    .then(function () {
      var asked = calls('session_task_stop')[0][1];
      check('Stop asks for the command by its id', asked.key === picked[0].key && asked.id === 'b70i34vi3', JSON.stringify(asked));
      check('and says it’s at it meanwhile', !!buttonIn(rows()[1], 'Stopping…') && buttonIn(rows()[1], 'Stopping…').disabled);
      stopped('Stopped: it and what it started have ended, and its agent is told so.');
      return until(function () { return rows()[1].querySelector('.ca-task-said'); }, 2000, 'what Stop said');
    })
    .then(function () {
      check('then says how it went', rows()[1].querySelector('.ca-task-said').textContent === 'Stopped: it and what it started have ended, and its agent is told so.');
      buttonIn(rows()[0], 'Output').click();
      return until(function () { return rows()[0].querySelector('.ca-task-output pre'); }, 2000, 'its output shown');
    })
    .then(function () {
      var r = rows()[0];
      var asked = calls('session_task_output').map(function (c) { return c[1]; });
      check('Output asks for the last of what it printed, by its id', asked.length >= 1 && asked[0].key === picked[0].key && asked[0].id === 'bfoz923q7', JSON.stringify(asked));
      check('and shows it as a terminal did', /VITE v5\.4\.0 {2}ready in 312 ms/.test(r.querySelector('pre').textContent), r.querySelector('pre').textContent);
      var link = r.querySelector('.ca-task-address');
      check('with a link to the address the server printed', !!link && link.textContent === 'localhost:4321/', link ? link.textContent : 'no link');
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
