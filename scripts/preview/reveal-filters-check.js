// Checks Show in Codealong on a session the main window's filters leave out (App.tsx `reveal`), as
// the quick panel asks for one it found: in another project than the window shows, it goes to
// that project; on a machine the sidebar's devices leave out, this Mac's or another's, they're
// cleared, rather than nothing happening.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/reveal-filters-check.js
(function () {
  // This runs before the app's script: the board's sessions are read here, so the window can open
  // on one session's project with another's to show.
  var req = new XMLHttpRequest();
  req.open('GET', './board.json', false);
  req.send();
  var board = JSON.parse(req.responseText);
  var listed = function (l) { return !l.hidden && !l.device; };
  var first = function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived && !(s.helper && s.helper.parent); })[0]; };
  var pick = function (lanes) {
    for (var i = 0; i < lanes.length; i++) {
      var s = first(lanes[i]);
      if (s) return { lane: lanes[i].id, key: s.key, project: lanes[i].project };
    }
    return null;
  };
  var a = pick(board.lanes.filter(listed));
  var b = a && pick(board.lanes.filter(function (l) { return listed(l) && l.project !== a.project; }));
  localStorage.setItem('codealong.age', 'all');
  if (a) localStorage.setItem('codealong.project', a.project);

  var row = function (x) { return document.getElementById('row-' + x.lane + '::' + x.key); };
  var shown = function (x) { var r = row(x); return !!r && r.getAttribute('aria-selected') === 'true'; };
  var devRows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-side button.ca-side-devrow')); };
  var pressed = function () { return devRows().filter(function (r) { return r.getAttribute('aria-pressed') === 'true'; }).length; };
  var reveal = function (x) {
    window.__emit('reveal', { kind: 'session', laneId: x.lane, key: x.key });
    return until(function () { return shown(x); }, 3000).catch(function () {});
  };
  var c = null;

  say('reveal filters check: waiting for the board');
  if (!a || !b) {
    check('two sessions on this Mac in different projects', false, a ? 'one project' : 'no session');
    done();
    return;
  }
  until(function () { return !!document.querySelector('[role="listbox"]') && devRows().length >= 2; }, 20000)
    .catch(function () { throw new Error('no list, or no linked machines (run with PREVIEW_DEVICES)'); })
    .then(function () { return window.__TAURI_INTERNALS__.invoke('devices'); })
    .then(function (d) {
      c = pick(d.lanes);
      check('the window opens on the first session’s project', localStorage.getItem('codealong.project') === JSON.stringify(a.project) || localStorage.getItem('codealong.project') === a.project, localStorage.getItem('codealong.project'));
      return reveal(b);
    })
    .then(function () {
      check('a session in another project shows', shown(b), row(b) ? row(b).outerHTML.slice(0, 160) : 'no row');
      var p = localStorage.getItem('codealong.project');
      check('in its project', p === b.project || p === JSON.stringify(b.project), p);
      // Mac mini alone: this Mac's sessions aren't listed.
      devRows()[1].click();
      return until(function () { return pressed() === 1 && !row(a); }, 3000).catch(function () {});
    })
    .then(function () {
      check('the machine filter leaves this Mac’s session out', pressed() === 1 && !row(a), pressed() + ' picked');
      return reveal(a);
    })
    .then(function () {
      check('a session on this Mac shows under a filter that left it out', shown(a), row(a) ? row(a).outerHTML.slice(0, 160) : 'no row');
      check('with the machine filter cleared', pressed() === 0, pressed() + ' picked');
      if (!c) {
        check('a session on another machine', false, 'no linked machine’s sessions');
        return;
      }
      // This Mac alone: the linked machine's sessions aren't listed.
      devRows()[0].click();
      return until(function () { return pressed() === 1; }, 3000)
        .catch(function () {})
        .then(function () { return reveal(c); })
        .then(function () {
          check('a session on another machine shows under a filter that left it out', shown(c), row(c) ? row(c).outerHTML.slice(0, 160) : 'no row');
          check('with the machine filter cleared too', pressed() === 0, pressed() + ' picked');
        });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
