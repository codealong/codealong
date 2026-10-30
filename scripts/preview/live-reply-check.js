// Checks the reply a Claude Code session is writing, handed over by the plugin's MessageDisplay
// hook (claude/live.rs) before its record has it (SessionThread.tsx `stream`): a session waiting
// on a question shows what it wrote with it at the end of its conversation, under its account
// rather than "writing", and the question no longer says where to read it. A later batch grows
// it, another session's doesn't touch it, and once the record has the message it shows once,
// from the record. A session that's working says it's writing. A change of status asks again for
// what it's writing, as a session on another device pushes nothing here.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/live-reply-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var status = 'needs';
  var written = false;
  var TEXT = 'Your first three answers are recorded. Cloud SQL proxy and Qdrant already have fixes available.';
  var MORE = TEXT + '\n\n- **Qdrant** needs our own rebuild.';
  var pending = {
    id: 'ask-9',
    label: 'Asks you · Which plans do you approve?',
    input: null,
    cannot: null,
    questions: [{ question: 'Which plans do you approve?', header: 'Plans', options: [{ label: 'Cloud SQL proxy 2.26.2' }, { label: 'Qdrant v1.19.2 rebuild' }], multiSelect: true }],
    unwritten: false,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_pending' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(status === 'needs' ? pending : null);
    }
    if (cmd === 'session_stream' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(written ? null : window.__stream || null);
    }
    // The record has the message once it's written, after the page the panel opened on.
    if (cmd === 'session_newer' && mine) {
      window.__calls.push([cmd, args, options]);
      var after = (args || {}).after || 0;
      if (!written || after >= 100) return Promise.resolve({ items: [], before: null, after: after });
      return Promise.resolve({ items: [{ at: Date.now(), kind: 'agent', text: MORE, failed: false, cursor: 100, n: 0 }], before: null, after: 100 });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(shaped);
  };
  var shaped = function (board) {
    board.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
        s.snoozedUntil = undefined;
        s.raised = undefined;
        if (picked ? s.key !== picked.key : l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
        s.status = status;
        s.statusSince = Date.now() - 60e3;
        s.asks = false;
        s.hosted = false;
        s.excerpt = status === 'needs' ? pending.label : 'Writing';
        // Each board a change of the session, as the engine sends one.
        s.lastActivity = Date.now();
        if (!picked) {
          picked = { key: s.key, lane: l.id, account: s.account };
          window.__stream = { key: s.key, text: TEXT, tool: null };
        }
      });
    });
    return board;
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var said = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-said.is-agent')); };
  var last = function () { var all = said(); return all[all.length - 1]; };
  var lastText = function () { var l = last(); var body = l && l.querySelector('.ca-md, .ca-thread-text'); return ((body || l || {}).textContent || '').trim(); };
  var times = function (words) { return said().filter(function (s) { return s.textContent.indexOf(words) >= 0; }).length; };
  var labelOf = function (el) { var m = el && el.querySelector('.ca-thread-meta > span'); return m ? m.textContent : null; };
  var push = function () { return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); }); };

  say('live reply check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return times('Your first three answers') > 0 && document.querySelector('.ca-sessinsp .ca-composer.is-questions'); }, 8000);
    })
    .then(function () {
      check('what it wrote with the question is at the end', /^Your first three answers are recorded/.test(lastText()), lastText());
      var label = labelOf(last());
      check('under its account, not writing', label === null || label.indexOf('writing') < 0, String(label));
      check('the question no longer says where to read it', !document.querySelector('.ca-sessinsp .ca-approval-unwritten'));
      window.__emit('hosted-stream', { key: picked.key, text: MORE, tool: null });
      window.__emit('hosted-stream', { key: picked.key + '-other', text: 'Another session.', tool: null });
      return until(function () { return /Qdrant needs our own rebuild/.test(lastText()); }, 3000);
    })
    .then(function () {
      check('a later batch grows it', /Qdrant needs our own rebuild/.test(lastText()), lastText());
      check('another session’s stays out', times('Another session.') === 0);
      // Answered: the record has the message, and the engine no longer streams it.
      written = true;
      status = 'working';
      var asked = window.__calls.filter(function (c) { return c[0] === 'session_stream'; }).length;
      return push().then(function () {
        return until(function () { return window.__calls.filter(function (c) { return c[0] === 'session_stream'; }).length > asked && times('Your first three answers') === 1; }, 5000);
      });
    })
    .then(function () {
      check('asked again as the record grew', true);
      check('shows once, from the record', times('Your first three answers') === 1, times('Your first three answers') + ' times');
      // Writing the next one, with no batch pushed here: the next change of the session finds it.
      written = false;
      window.__stream = { key: picked.key, text: 'Now updating the plans.', tool: null };
      return push().then(function () {
        return until(function () { return times('Now updating the plans.') === 1; }, 5000);
      });
    })
    .then(function () {
      var label = labelOf(last());
      check('a working session says it’s writing', label === null || label === picked.account + ' · writing', String(label));
      done();
    })
    .catch(function (e) {
      check('no error', false, String(e && e.stack || e));
      done();
    });
})();
