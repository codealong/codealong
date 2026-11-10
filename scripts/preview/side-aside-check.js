// Checks setting a session aside for its side conversation in Sessions (SessionReply.tsx
// `onAsked`, SessionPanel.tsx, useWaitingQueue.ts; board.rs `apply_aside`): two sessions that ask
// you a question and one whose turn ended. ⌘⏎ on `/side <question>` in A's box asks a copy, sets A
// aside and goes on to B, the strip saying so, and A is listed with what works, drawn waiting and
// labelled On the side; ⌘E on B does nothing while nothing answers on its side, and once a question
// asked there with ⏎ is answering, sets B aside and goes on to C; the side box says what ⌘⏎ does.
// Once the engine's board says A is set aside it stays so; once it no longer does, A needs you
// again, back under Needs you.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/side-aside-check.js
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
    label: 'Asks you · Restore test?',
    input: null,
    cannot: null,
    questions: [{ question: 'Run the restore test this week?', header: 'Restore test', options: [{ label: 'This week' }, { label: 'Next quarter' }] }],
  };
  var picked = [];
  // When the engine's board has A set aside, as it would once its side conversation answers.
  window.__asideA = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked.slice(0, 2).some(function (p) { return p.key === (args || {}).key; })) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    if (cmd === 'aside_sessions') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(null);
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
          s.aside = undefined;
          var i = picked.findIndex(function (x) { return x.key === s.key; });
          if (i < 0 && (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude' || seen[s.key] || picked.length >= plan.length)) return;
          seen[s.key] = true;
          if (i < 0) {
            i = picked.length;
            picked.push({ key: s.key, lane: l.id });
          }
          var p = plan[i];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.lastPromptAt = p.since - 60e3;
          s.need = p.status === 'needs' ? 'question' : undefined;
          s.excerpt = p.status === 'needs' ? 'Asks you · Run the restore test this week?' : 'Done.';
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.live = true;
          s.title = 'Aside ' + 'ABC'[i];
          if (i === 0 && window.__asideA) Object.assign(s, { aside: window.__asideA, excerpt: 'Asking on the side · Are we restoring for real?' });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer:not(.is-questions) textarea'); };
  var said = function () { var c = document.querySelector('.ca-sessinsp .ca-composer:not(.is-questions)'); return c ? c.textContent : ''; };
  var calls = function (name) { return window.__calls.filter(function (c) { return c[0] === name; }).map(function (c) { return c[1]; }); };
  var asides = function () { return calls('aside_sessions').map(function (a) { return a.keys.join(','); }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  // The heading of the section a row is listed in.
  var under = function (el) {
    var block = el && el.closest('.ca-bayblock');
    var head = block && block.querySelector('.ca-bay');
    return head ? head.textContent.trim() : '';
  };
  var waitingGlyph = function (el) { var g = el.querySelector('.ca-glyph--waiting'); return !!g && g.getAttribute('aria-label') === 'Waiting on its side conversation'; };

  (async function () {
    try {
      say('side aside check: waiting for the list');
      await until(function () { return picked.length === plan.length && row(0) && row(1) && row(2); }, 20000, 'the three rows');
      await wait(300);
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await on('Aside A');
      await until(box, 5000, 'A’s box');
      check('a session that asks you a question starts under Needs you', /Needs you/.test(under(row(0))), under(row(0)));

      box().focus();
      type(box(), '/side Are we restoring for real?');
      await wait(50);
      key(box(), { key: 'Enter', metaKey: true });
      await on('Aside B');
      var asked = calls('side_ask');
      check('⌘⏎ on /side asks a copy of A', asked.length === 1 && asked[0].key === picked[0].key && asked[0].text === 'Are we restoring for real?', JSON.stringify(asked));
      check('then sets A aside', asides().join('|') === picked[0].key, asides().join('|'));
      check('and goes on to the next that waits', title() === 'Aside B', title());
      check('saying so', /Aside A: set aside\./.test(strip()), strip());
      await until(function () { return row(0) && /On the side/.test(row(0).textContent); }, 3000, 'A labelled On the side').catch(function () {});
      check('A is labelled On the side', /On the side/.test(row(0).textContent), row(0).textContent);
      check('drawn working, waiting', waitingGlyph(row(0)));
      check('and listed with what works', /Working/.test(under(row(0))), under(row(0)));

      // ⌘E on B with nothing answering on its side: it waits on its question still.
      await until(box, 5000, 'B’s box');
      key(box(), { key: 'e', metaKey: true });
      await wait(300);
      check('⌘E on a question with nothing on the side stays', title() === 'Aside B' && asides().length === 1, title() + ' ' + asides().join('|'));

      // Asked on the side with ⏎, B stays, its box the side conversation's, saying what ⌘⏎ does.
      box().focus();
      type(box(), '/side What else is due?');
      await wait(50);
      key(box(), { key: 'Enter' });
      await until(function () { return document.querySelector('.ca-sessinsp .ca-composer.is-aside'); }, 5000, 'B’s box the side conversation’s');
      check('⏎ asks and stays', title() === 'Aside B' && asides().length === 1, title());
      check('the side box says ⌘⏎ goes on', /asks and goes on/.test(said()), said());
      key(box(), { key: 'e', metaKey: true });
      await on('Aside C');
      check('⌘E while it answers sets B aside', asides().join('|') === picked[0].key + '|' + picked[1].key, asides().join('|'));
      check('and goes on to the next', title() === 'Aside C', title());

      // The engine's board says A is set aside: it stays so once the window's own change goes.
      window.__asideA = Date.now();
      window.__pushSides();
      await wait(1500);
      check('set aside by the board, A stays with what works', /Working/.test(under(row(0))) && /On the side/.test(row(0).textContent), under(row(0)));
      // Its side conversation done, the board no longer says so: it needs you again.
      window.__asideA = null;
      window.__pushSides();
      await until(function () { return /Needs you/.test(under(row(0))); }, 70000, 'A back under Needs you').catch(function () {});
      check('once that’s done, A needs you again', /Needs you/.test(under(row(0))) && !/On the side/.test(row(0).textContent), under(row(0)));
    } catch (e) {
      check('ran', false, String(e) + ' at ' + title());
    }
    done();
  })();
})();
