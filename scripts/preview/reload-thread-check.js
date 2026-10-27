// Checks ⌘R (reload.ts) on a session's conversation in Sessions (SessionThread.tsx): a
// conversation that was read asks only for what's new, leaving the view where it is, and only the
// latest of those reads says whether it's updating. A read asked before the reload that fails
// after the reload's read came back raises no notice, and one that comes back after the reload's
// failed doesn't take that notice away.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/reload-thread-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var now = Date.now();
  var picked = null;
  var first = {
    items: [
      { at: now - 30 * 60e3, kind: 'you', text: 'Tidy up the northwind importer.', failed: false, cursor: 1, n: 0 },
      { at: now - 29 * 60e3, kind: 'agent', text: 'The northwind importer reads each row and writes it back.', failed: false, cursor: 2, n: 0 },
    ],
    before: null,
    after: 2,
  };
  // Each read of what's new, answered when the check says.
  var newer = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_page' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(JSON.parse(JSON.stringify(first)));
    }
    if (cmd === 'session_newer' && mine) {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (resolve, reject) {
        newer.push({
          ok: function () { resolve({ items: [], before: null, after: 2 }); },
          fail: function () { reject(new Error('the record couldn’t be read')); },
        });
      });
    }
    if (cmd === 'session_stream' && mine) return Promise.resolve(null);
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'working') s.status = 'idle';
          if (picked ? s.key !== picked.key : l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          s.status = 'idle';
          s.lastActivity = now - 20 * 60e3;
          s.hosted = false;
          s.title = 'Reload me';
          if (!picked) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var notified = function () { return document.body.textContent.indexOf('A conversation isn’t updating') >= 0; };
  var reload = function () { document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', code: 'KeyR', metaKey: true, bubbles: true, cancelable: true })); };
  var asked = function (n) { return until(function () { return newer.length >= n; }, 3000, 'read of what’s new #' + n); };
  var pages;

  say('reload thread check: waiting for the list');
  until(function () { return picked && q('#row-' + CSS.escape(picked.lane + '::' + picked.key)); }, 20000, 'the session’s row')
    .then(function () {
      q('#row-' + CSS.escape(picked.lane + '::' + picked.key)).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return thread() && /northwind importer reads/.test(thread().textContent); }, 8000, 'the conversation');
    })
    .then(function () { return asked(1); })
    .then(function () {
      pages = calls('session_page').length;
      reload();
      return asked(2);
    })
    .then(function () {
      check('⌘R asks for what’s new', newer.length === 2, String(newer.length));
      check('and doesn’t read the conversation from the start, which worked', calls('session_page').length === pages, pages + ' → ' + calls('session_page').length);
      newer[1].ok();
      return wait(100);
    })
    .then(function () {
      newer[0].fail();
      return wait(300);
    })
    .then(function () {
      check('a read from before the reload failing after it raises no notice', !notified());
      reload();
      return asked(3);
    })
    .then(function () {
      reload();
      return asked(4);
    })
    .then(function () {
      newer[3].fail();
      return until(notified, 3000, 'the notice').catch(function () {});
    })
    .then(function () {
      check('the latest read failing says so', notified());
      newer[2].ok();
      return wait(300);
    })
    .then(function () {
      check('and an earlier read coming back after it doesn’t take that away', notified());
      done();
    })
    .catch(stopped);
})();
