// Checks Mark as done on a session whose helpers' work isn't over (menus.ts `doneHelpers`,
// ui/DoneHelpers.tsx): one session waits on you, with three helpers under it, one working, one
// waiting on you and one done. ⇧⌘E from its panel asks nothing of the engine yet: a dialog lists
// the two whose work isn't over (not the one that's done), says what each choice does, and has
// Make them my own as its default. Cancel leaves everything as it was. Asked again, End them asks
// the engine to end those two, by their keys (which holds the session here, for a command left
// running, so it stays to be asked about again); asked a third time, the default button makes
// them yours, naming them to keep.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/done-helpers-check.js
//   PREVIEW_WAIT=60000 PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/done-helpers-check.js
(function () {
  // This runs before the app's script: the board it gets has one session waiting on you, made
  // from a real one on this device, and three of the next ones its helpers.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
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
    if (cmd === 'session_done') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ done: false, message: '“Ship the release” left `npm run dev` running in the background.' });
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
          if (s.helper && !picked.some(function (p) { return p.key === s.key; })) s.helper = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.title = p.title;
          s.asks = false;
          s.live = true;
          s.background = undefined;
          s.agents = [];
          s.orchestrator = undefined;
          s.excerpt = p.status === 'needs' ? 'Bash · `npm install`' : s.excerpt;
          s.helper = picked.length === 0 ? undefined : { parent: picked[0].key, report: true };
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var dialog = function () { return document.querySelector('.ca-donehelpers'); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var doneKey = function () { key({ key: 'E', code: 'KeyE', metaKey: true, shiftKey: true }); };
  var asked = function () { return window.__calls.filter(function (c) { return c[0] === 'session_done'; }).map(function (c) { return c[1]; }); };
  var button = function (label) {
    var d = dialog();
    return d && Array.prototype.filter.call(d.querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0];
  };
  var open = function () {
    row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === 'Ship the release'; }, 3000)
      .then(function () { return wait(50); })
      .then(function () {
        doneKey();
        return until(dialog, 3000, 'the dialog');
      })
      .then(function () { return wait(100); });
  };

  say('done helpers check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(open)
    .then(function () {
      var d = dialog();
      var rows = Array.prototype.map.call(d.querySelectorAll('.ca-donehelpers-title'), function (t) { return t.textContent; });
      var lede = d.querySelector('.ca-dialog-lede');
      check('⇧⌘E asks nothing of the engine yet', asked().length === 0, JSON.stringify(asked()));
      check('the dialog lists the helpers whose work isn’t over, not the one that’s done', rows.join('|') === 'Bump dependencies|Bring the docs level', rows.join('|'));
      check('it says whose they are and what each choice does', /“Ship the release” has 2 helpers whose work isn’t over\. Make them your own .* or end them too/.test(d.textContent), lede ? lede.textContent : d.textContent);
      check('each says where it stands', /working for 4m/.test(d.textContent) && /waits on you/.test(d.textContent), d.textContent);
      var mine = button('Make them my own');
      check('Make them my own is the default', !!mine && document.activeElement === mine, document.activeElement ? document.activeElement.textContent : 'nothing');
      note('the dialog, in front of the panel');
      return wait(200);
    })
    .then(function () {
      button('Cancel').click();
      return wait(200);
    })
    .then(function () {
      check('Cancel closes it and does nothing', !dialog() && asked().length === 0 && title() === 'Ship the release', title());
      return open();
    })
    .then(function () {
      button('End them').click();
      return until(function () { return asked().length === 1; }, 3000, 'the engine asked');
    })
    .then(function () {
      var a = asked()[0];
      var named = a && (a.endHelpers || []).join('|');
      check('End them asks the engine to end those two, by their keys', !!a && a.key === picked[0].key && named === picked[1].key + '|' + picked[2].key && (a.keepHelpers || []).length === 0 && a.stop === false, JSON.stringify(a));
      check('and the dialog goes', !dialog());
      return wait(300);
    })
    .then(open)
    .then(function () {
      // The default, the button that has the keyboard as it opens.
      document.activeElement.click();
      return until(function () { return asked().length === 2; }, 3000, 'the engine asked again');
    })
    .then(function () {
      var a = asked()[1];
      var kept = a && (a.keepHelpers || []).join('|');
      check('Make them my own: done, keeping those two by their keys', !!a && a.key === picked[0].key && (a.endHelpers || []).length === 0 && kept === picked[1].key + '|' + picked[2].key, JSON.stringify(a));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
