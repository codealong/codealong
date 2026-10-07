// Checks File › Close idle sessions… (ui/CloseIdleDialog.tsx) on a plan of its own: a session in
// a terminal tab and a background job are listed, each saying what closing does to it, and an
// app's session stays open. Also archive them asks the engine again, lists the app's session to
// be archived only, and is remembered; the button says what it will do to what's ticked, the
// engine is asked to close the ticked ones and archive them, and the toast counts what was
// closed, stopped and archived.
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/close-idle-check.js
(function () {
  localStorage.removeItem('codealong.closeIdle.archive');
  var now = Date.now();
  var row = function (key, title, ends, why) {
    return { key: key, title: title, account: 'claude', harness: 'claude', ends: ends, pid: ends === 'terminal' ? 41 : null, cwd: '/work/app', worktree: '/work/app', callsign: 'app', lastActivity: now - 2 * 864e5, why: why };
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'idle_sessions') {
      window.__calls.push([cmd, args, options]);
      var close = [row('t1', 'Tidy the README', 'terminal', 'idle 2 days · last reply asks nothing'), row('j1', 'Nightly sweep', 'job', 'idle 2 days · last reply asks nothing')];
      var kept = [{ key: 'w1', title: 'Fix login', account: 'claude', why: 'working' }];
      var app = { key: 'a1', title: 'RFC review', account: 'claude' };
      if (args.archive) close.push(row('a1', app.title, 'archive', 'idle 2 days · last reply asks nothing · stays open in the Claude app'));
      else kept.unshift(Object.assign({ why: 'in the Claude app' }, app));
      return Promise.resolve({ afterMs: args.afterMs, archive: args.archive, close: close, kept: kept });
    }
    if (cmd === 'close_idle_sessions') {
      window.__calls.push([cmd, args, options]);
      var ends = { t1: 'terminal', j1: 'job', a1: 'archive' };
      return Promise.resolve(
        args.keys.map(function (k) {
          return { key: k, title: k, ends: ends[k], closed: ends[k] !== 'archive', archived: args.archive, ok: true, message: '' };
        }),
      );
    }
    return real(cmd, args, options);
  };

  var dialog = function () { return q('.ca-idle'); };
  var rows = function () { return all('.ca-idle-list .ca-idle-row'); };
  var does = function () { return all('.ca-idle-does').map(function (d) { return d.textContent; }).join(' | '); };
  var button = function () { return all('.ca-dialog-foot button').pop(); };
  var label = function () { var b = button(); return b ? b.textContent : ''; };
  var note = function () { var n = q('.ca-idle-note'); return n ? n.textContent : ''; };
  var archiveBox = function () { return q('.ca-idle-archive input'); };
  var boxOf = function (i) { return rows()[i].querySelector('input'); };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  // The menu item reaches the window as an event, heard once the window listens for it.
  var open = function () {
    return until(function () {
      window.__emit('close-idle');
      return dialog();
    }, 20000, 'the dialog');
  };

  say('close-idle check: opening');
  open()
    .then(function () { return until(function () { return rows().length === 2; }, 3000, 'two rows'); })
    .then(function () {
      check('it’s called Close idle sessions', /Close idle sessions/.test(q('.ca-dialog-head').textContent), q('.ca-dialog-head').textContent);
      check('the plan is asked without archiving', last('idle_sessions').archive === false, JSON.stringify(last('idle_sessions')));
      check('a tab and a job are listed, each saying what closing does', does() === 'Closes its tab | Stops its job', does());
      check('the button closes both', label() === 'Close 2 sessions', label());
      check('what stays open says where', note() === 'Stay open: 1 in the Claude app, 1 working.', note());
      check('Also archive them starts unticked', archiveBox() && !archiveBox().checked);
      archiveBox().click();
      return until(function () { return rows().length === 3; }, 3000, 'three rows');
    })
    .then(function () {
      check('ticking it asks the plan again, archiving', last('idle_sessions').archive === true, JSON.stringify(last('idle_sessions')));
      check('and is remembered', localStorage.getItem('codealong.closeIdle.archive') === '1', localStorage.getItem('codealong.closeIdle.archive'));
      check('the app’s session is listed to be archived only', does() === 'Closes its tab | Stops its job | Only archived', does());
      check('saying where it stays open', /stays open in the Claude app/.test(rows()[2].textContent), rows()[2].textContent);
      check('the button closes and archives them', label() === 'Close and archive 3 sessions', label());
      check('only the working one stays open', note() === 'Stay open: 1 working.', note());
      boxOf(0).click();
      boxOf(1).click();
      return wait(50);
    })
    .then(function () {
      check('with only that one ticked, the button archives it', label() === 'Archive 1 session', label());
      check('the count says what’s ticked', /1 of 3 ticked/.test(q('.ca-idle-count').textContent), q('.ca-idle-count').textContent);
      boxOf(0).click();
      boxOf(1).click();
      return wait(50);
    })
    .then(function () {
      button().click();
      return until(function () { return !dialog(); }, 3000, 'the dialog to close');
    })
    .then(function () {
      var asked = last('close_idle_sessions');
      check('the engine is asked to close the ticked ones and archive them', !!asked && asked.keys.join() === 't1,j1,a1' && asked.archive === true && asked.afterMs === 864e5, JSON.stringify(asked));
      return until(toast, 3000, 'the toast');
    })
    .then(function () {
      check('the toast counts what was closed, stopped and archived', /Closed 1 session, stopped 1 background job and archived 3\./.test(toast()), toast());
      return open();
    })
    .then(function () { return until(function () { return rows().length === 3; }, 3000, 'three rows again'); })
    .then(function () {
      check('opened again, it still archives', archiveBox().checked && last('idle_sessions').archive === true, JSON.stringify(last('idle_sessions')));
    })
    .then(done)
    .catch(stopped);
})();
