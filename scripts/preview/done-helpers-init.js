// Opens Mark as done's question about helpers whose work isn't over on a session with three
// helpers, one working, one waiting on you and one done, for a screenshot of it
// (done-helpers-check.js checks what it does).
//   PREVIEW_INIT=scripts/preview/done-helpers-init.js PREVIEW_NAME=done-helpers scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var now = Date.now();
  var plan = [
    { title: 'Ship the release', status: 'unread', since: now - 10 * 60e3 },
    { title: 'Bump dependencies', status: 'working', since: now - 4 * 60e3 },
    { title: 'Bring the docs level', status: 'needs', since: now - 2 * 60e3 },
    { title: 'Read the performance log', status: 'idle', since: now - 30 * 60e3 },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          if (s.helper && !picked.some(function (p) { return p.key === s.key; })) s.helper = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.title = p.title;
          s.live = true;
          s.background = undefined;
          s.agents = [];
          s.orchestrator = undefined;
          s.helper = picked.length === 0 ? undefined : { parent: picked[0].key, report: true };
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };
  var step = 0;
  var t = setInterval(function () {
    var row = picked.length === plan.length && document.getElementById('row-' + picked[0].lane + '::' + picked[0].key);
    if (step === 0 && row) {
      step = 1;
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return;
    }
    var title = document.querySelector('.ca-sessinsp-title');
    if (step === 1 && title && title.textContent === 'Ship the release') {
      step = 2;
      clearInterval(t);
      (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { key: 'E', code: 'KeyE', metaKey: true, shiftKey: true, bubbles: true, cancelable: true }));
    }
  }, 100);
})();
