// Checks that the quick panel's rows hold still as Jev's suggestions come in (Quick.tsx, aim.ts):
// Suggested is there, saying Jev is working, before the first answer; at the next key the last
// answer's suggestions stay where they were, dimmed, without keys and doing nothing when clicked,
// and the rows under them don't move, then or when the next answer comes; and a click on a row
// that came under a pointer that stayed put only selects it, the next click doing it, while a
// click on a row the pointer was moved onto, or that a scroll brought under it, does it at once.
//   PREVIEW_ROUTE=scripts/preview/route-late-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-steady-check.js
// The board needs a session in a worktree that isn't hidden.
(function () {
  // This runs before the app's script: one of the board's real sessions gets a title nothing
  // else has, so a search finds it by that, and the session Jev sends to (the stub's
  // "$session:0": the latest prompted of those running and not waiting on you, in a worktree
  // that isn't hidden) is found by what was said in it alone, so it's listed under the first
  // until Jev's answer puts it first.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var TITLE = 'Steady the zebrafinch and the marmoset';
  var FIRST = 'zebrafinch marmoset';
  var NEXT = 'zebrafinch marmoset zebrafinch';
  var picked = null;
  var jevs = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') {
      return Promise.resolve({ hits: jevs ? [{ key: jevs.key, snippet: '…so the zebrafinch and the marmoset both go through the same path.', you: false, at: jevs.lastActivity }] : [], building: false, off: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var lanes = board.lanes.filter(function (l) { return !l.hidden; }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
      var live = lanes.flatMap(function (l) { return l.sessions.filter(function (s) { return !s.visitor && s.live && s.status !== 'needs'; }); }).sort(function (a, b) { return b.lastPromptAt - a.lastPromptAt; });
      jevs = jevs || live[0] || null;
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked ? picked !== s.key : l.hidden || s.visitor || s.archived || (jevs && s.key === jevs.key)) return;
          s.title = TITLE;
          picked = s.key;
        });
      });
      return board;
    });
  };

  var input = function () { return q('.ca-quick-input textarea'); };
  // The session the search finds, not a suggestion about it.
  var found = function () { return all('.ca-quick-row:not(.is-suggest)').filter(function (r) { return r.textContent.indexOf(TITLE) >= 0; })[0]; };
  var fresh = function () { return all('.ca-quick-row.is-suggest:not(.is-stale):not(.is-note)'); };
  var stale = function () { return all('.ca-quick-row.is-stale'); };
  var top = function (el) { return Math.round(el.getBoundingClientRect().top); };
  var hides = function () { return calls('hide_quick').length; };
  // A mouse event where the pointer already is: what a browser may send as a row comes under a
  // pointer that stayed put.
  var still = function (el, at) { el.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, cancelable: true, clientX: at.x, clientY: at.y, screenX: at.x, screenY: at.y })); };
  var click = function (el) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 })); };
  var foundAt, count, hidden, at;

  say('quick steady check: waiting for the list');
  until(function () { return !!picked && !!input() && all('.ca-quick-row').length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      type(input(), FIRST);
      return until(function () { return q('.ca-quick-row.is-note.is-routing') && found(); }, 3000, 'Suggested saying Jev is working, and the session found');
    })
    .then(function () {
      check('Suggested is there before Jev answers, saying it’s working', /Working out where this goes/.test(q('.ca-quick-row.is-note').textContent), q('.ca-quick-row.is-note').textContent);
      check('and nothing over the list says so instead', !q('.ca-quick-note.is-routing'));
      foundAt = top(found());
      return until(function () { return fresh().length > 0; }, 8000, 'Jev’s suggestions');
    })
    .then(function () {
      count = fresh().length;
      if (!jevs) note('no session running and not waiting on you, so none Jev puts first among those found');
      note(count + ' suggestions; the session found moved ' + (top(found()) - foundAt) + 'px as the first answer came');
      at = pointAt(found());
      return wait(50);
    })
    .then(function () {
      // A row that comes under a pointer that stayed put isn't the one you aimed at.
      check('a row the pointer is moved onto is selected', found().classList.contains('is-active'));
      still(fresh()[0], at);
      return wait(50);
    })
    .then(function () {
      check('a row that comes under a still pointer isn’t selected by it', !fresh()[0].classList.contains('is-active') && found().classList.contains('is-active'));
      hidden = hides();
      click(fresh()[0]);
      return wait(50);
    })
    .then(function () {
      check('a click on it only selects it', hides() === hidden && input().value === FIRST && fresh()[0].classList.contains('is-active'), 'hides ' + hidden + '→' + hides() + ', text “' + input().value + '”');
      click(fresh()[0]);
      return wait(50);
    })
    .then(function () {
      check('and the next click does it', hides() === hidden + 1 && input().value === '', 'hides ' + hidden + '→' + hides() + ', text “' + input().value + '”');
      // Jev's answer for what was typed first comes back at once now (the panel keeps it).
      type(input(), FIRST);
      return until(function () { return fresh().length === count && found(); }, 3000, 'the first answer again');
    })
    .then(function () {
      foundAt = top(found());
      type(input(), NEXT);
      return wait(100);
    })
    .then(function () {
      check('at the next key the last suggestions stay', stale().length === count && fresh().length === 0, stale().length + ' stale, ' + fresh().length + ' fresh');
      check('without their keys', stale().every(function (r) { return !r.querySelector('.ca-kbd'); }));
      check('the rows under them don’t move', found() && top(found()) === foundAt, found() && top(found()) + ' vs ' + foundAt);
      check('Suggested’s heading says Jev is working', !!q('.ca-quick-glabel-routing'));
      pointAt(stale()[0]);
      hidden = hides();
      click(stale()[0]);
      return wait(50);
    })
    .then(function () {
      check('a click on a stale suggestion does nothing', hides() === hidden && input().value === NEXT, 'hides ' + hidden + '→' + hides() + ', text “' + input().value + '”');
      return until(function () { return fresh().length > 0; }, 8000, 'the next answer');
    })
    .then(function () {
      check('the next answer takes their place', stale().length === 0 && fresh().length === count, fresh().length + ' vs ' + count);
      check('and the rows under it still don’t move', top(found()) === foundAt, top(found()) + ' vs ' + foundAt);
      pointAt(found());
      return wait(50);
    })
    .then(function () {
      hidden = hides();
      var shown = calls('show_main').length;
      click(found());
      check('a click on a row the pointer was moved onto does it at once', hides() === hidden + 1 && calls('show_main').length === shown + 1);
      type(input(), FIRST);
      return until(function () { return fresh().length === count && found(); }, 3000, 'the first answer again');
    })
    .then(function () {
      // Scrolling moves rows under a still pointer too, but that's yours: the row it brings there
      // is the one you aim at.
      at = pointAt(q('.ca-quick-compose'));
      return wait(50);
    })
    .then(function () {
      q('.ca-quick-list').dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: -40 }));
      still(found(), at);
      return wait(50);
    })
    .then(function () {
      check('a row a scroll brings under the pointer is selected', found().classList.contains('is-active'));
      hidden = hides();
      click(found());
      check('and a click does it at once', hides() === hidden + 1);
      // A word is a search, not for Jev.
      type(input(), 'zebrafinch');
      return until(function () { return found() && !q('.ca-quick-glabel.is-suggested'); }, 3000, 'a search of one word');
    })
    .then(function () {
      at = pointAt(found());
      return wait(50);
    })
    .then(function () {
      // A scroll, then rows moved by themselves, keeping their keys: Suggested comes in over
      // them as the search becomes a few words for Jev.
      q('.ca-quick-list').dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: -40 }));
      type(input(), 'zebrafinch zebrafinch');
      return until(function () { return q('.ca-quick-row.is-note') && found(); }, 3000, 'Suggested over the search');
    })
    .then(function () {
      still(found(), at);
      hidden = hides();
      click(found());
      return wait(50);
    })
    .then(function () {
      check('a scroll aims no more once rows have moved by themselves', hides() === hidden && found().classList.contains('is-active'), 'hides ' + hidden + '→' + hides());
    })
    .then(done)
    .catch(stopped);
})();
