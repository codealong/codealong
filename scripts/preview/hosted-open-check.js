// Checks the Open button in the header of a session Codealong runs (SessionHeader.tsx `shownHere`).
// In the main window's panel, which is where it lives, the button doesn't offer to show it there:
// it leads with Open folder in your editor and opens the folder in it, and its menu has no Show in
// Codealong. A session that runs elsewhere still leads with its own place. In the peek, the button
// says Show in Codealong, which brings the panel up.
//   scripts/preview.sh sessions '' scripts/preview/hosted-open-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/hosted-open-check.js
(function () {
  var peek = !!window.__CODEALONG_PEEK__;
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');

  // This runs before the app's script: two sessions in worktrees on this Mac, the first made one
  // Codealong runs and the second left where it runs, both the latest so Sessions lists them.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = [];
  board.lanes.forEach(function (l) {
    if (l.hidden || l.device || !l.targets.some(function (t) { return t.id === 'l|editor'; })) return;
    l.sessions.forEach(function (s) {
      if (picked.length < 2 && !s.archived && !s.visitor && !s.hosted && picked.every(function (p) { return p.lane !== l.id; })) picked.push({ key: s.key, lane: l.id });
    });
  });
  if (picked.length < 2) { stopped('no two sessions in worktrees on this Mac'); return; }
  var hosted = picked[0].key;
  var apps = board.apps;
  localStorage.setItem('codealong.preview.peek', hosted);
  var now = Date.now();
  var made = function (l, s) {
    var mine = picked.filter(function (p) { return p.key === s.key; })[0];
    if (!mine) return s;
    var out = Object.assign({}, s, { status: 'idle', asks: false, snoozedUntil: undefined, lastActivity: now, lastPromptAt: now, statusSince: now });
    if (s.key !== hosted) return out;
    return Object.assign(out, { hosted: true, live: true, pid: null, openTarget: 's|' + s.key + '|hosted' });
  };
  // Its worktree lists Show in Codealong for it in place of its own ways to open it, as board.rs
  // `targets` does.
  var targetsOf = function (l) {
    var own = 's|' + hosted + '|';
    var at = l.targets.findIndex(function (t) { return t.id.indexOf(own) === 0; });
    if (at < 0) return l.targets;
    var rest = l.targets.filter(function (t) { return t.id.indexOf(own) !== 0 || /\|chrome$/.test(t.id); });
    var show = { id: own + 'hosted', icon: 'app', label: 'Show in Codealong', detail: null, kbd: ['⏎'], disabled: false, group: 'session' };
    return rest.slice(0, at).concat([show], rest.slice(at));
  };
  var shaped = function (b) {
    b.lanes = b.lanes.map(function (l) { return Object.assign({}, l, { sessions: l.sessions.map(function (s) { return made(l, s); }), targets: targetsOf(l) }); });
    return b;
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    return cmd === 'get_board' ? p.then(shaped) : p;
  };

  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'open_target' || c[0] === 'open_from_bar'; }).map(function (c) { return c[1].targetId; }); };
  var main = function () { var b = document.querySelector('.ca-sessinsp-head .ca-split-main span'); return b ? b.textContent : null; };
  var items = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-sessinsp-head .ca-openin .ca-menu-item .ca-menu-label > span:first-child'), function (e) { return e.textContent; }); };
  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (p) { return document.getElementById('row-' + p.lane + '::' + p.key); };
  var titleOf = function (p) { var s = board.lanes.filter(function (l) { return l.id === p.lane; })[0].sessions.filter(function (s) { return s.key === p.key; })[0]; return s.title; };
  var select = function (p) {
    return until(function () { return row(p); }, 20000).then(function () {
      row(p).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      row(p).click();
      return until(function () { return title() === titleOf(p) && main(); }, 5000);
    });
  };

  if (peek) {
    say('hosted open check: waiting for the peek');
    until(main, 20000)
      .then(function () {
        check('the peek says Show in Codealong', main() === 'Show in Codealong', main());
        document.querySelector('.ca-sessinsp-head .ca-split-main').click();
        return wait(100);
      })
      .then(function () {
        check('which shows it in the main window', opens().pop() === 's|' + hosted + '|hosted', opens().join());
      })
      .catch(function (e) { check('ran', false, String(e)); })
      .then(done);
    return;
  }

  say('hosted open check: waiting for the list');
  select(picked[0])
    .then(function () {
      check('the main window leads with your editor', main() === 'Open folder in ' + apps.editor, main());
      document.querySelector('.ca-sessinsp-head .ca-split-caret').click();
      return wait(100);
    })
    .then(function () {
      var l = items();
      check('its menu has no Show in Codealong', l.length > 0 && !l.some(function (x) { return /in Codealong$/.test(x); }), l.join(' | '));
      check('and still the terminal and the editors', l.indexOf('Open folder in ' + apps.terminal) >= 0 && l.indexOf('Open folder in ' + apps.editor) >= 0, l.join(' | '));
      document.querySelector('.ca-sessinsp-head .ca-split-caret').click();
      document.querySelector('.ca-sessinsp-head .ca-split-main').click();
      return wait(100);
    })
    .then(function () {
      check('the button opens the folder in your editor', opens().pop() === 'l|editor|' + apps.editorId, opens().join());
      return select(picked[1]);
    })
    .then(function () {
      var lane = board.lanes.filter(function (l) { return l.id === picked[1].lane; })[0];
      var s = lane.sessions.filter(function (x) { return x.key === picked[1].key; })[0];
      var own = lane.targets.filter(function (t) { return t.id === s.openTarget; })[0];
      check('a session that runs elsewhere leads with its own place', main() !== null && main().indexOf('Open folder in ') !== 0 && (!own || main() === own.label), main() + ' / ' + (own && own.label));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
