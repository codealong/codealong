// Checks what a side conversation that answered does to its session in Sessions (side.ts
// `useSideAnswer`, `showAnswer`; board.rs `apply_side_answer` gives the board this check gives it):
// a window that comes to the session shows the one that answered above its box, closed or open,
// and the box goes to it; put away, it doesn't come back in that window, until another answer; an
// answer that comes in while the session shows doesn't take the box from whoever may be typing,
// until the window comes forward; once the session's seen, one that came up by itself goes, and
// the box is the session's again; and one you picked yourself stays.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/side-answer-check.js
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  // Two Claude Code sessions to go between.
  var rows = [];
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!s.archived && !l.hidden && !l.device && !s.visitor && !s.helper && !s.headless && s.harness === 'claude') rows.push({ lane: l, session: s }); }); });
  if (rows.length < 2) { stopped('fewer than two Claude Code sessions to go between'); return; }
  var a = rows[0];
  var b = rows[1];
  var key = a.session.key;
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.removeItem('codealong.draft.' + key);
  localStorage.removeItem('codealong.draft.side:' + key);
  localStorage.removeItem('codealong.sideview.' + key);
  var t = Date.now() - 60000;
  var turn = { question: 'Why a queue?', answer: 'Messages to one session arrive in the order you sent them.', done: true, at: t };
  window.__past = [{ id: 'closed-1', key: key, title: '', startedAt: t, lastAt: t, turns: [turn] }];
  window.__answer = { id: 'closed-1', at: t };
  var calm = { status: 'idle', live: true, stoppable: false, stalledSince: null, asks: false, snoozedUntil: null, background: undefined };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b2) {
      b2.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.key === b.session.key) Object.assign(s, calm);
          if (s.key !== key) return;
          Object.assign(s, calm);
          if (window.__answer) Object.assign(s, { status: 'unread', statusSince: window.__answer.at, excerpt: 'Answered on the side · Why a queue?', sideAnswer: window.__answer });
        });
      });
      return b2;
    });
  };
  var row = function (r) { return document.getElementById('row-' + r.lane.id + '::' + r.session.key); };
  var strip = function () { return q('.ca-sidestrip'); };
  var aside = function () { return !!q('.ca-composer.is-aside'); };
  var acts = function (label) { return all('.ca-sidestrip-head button', strip()).filter(function (el) { return el.getAttribute('aria-label') === label; })[0]; };
  var go = async function (r) {
    row(r).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    await until(function () { return row(r).classList.contains('is-selected'); }, 5000, 'its row selected');
    await wait(100);
  };

  (async function () {
    try {
      await until(function () { return row(a) && row(b); }, 30000, 'the two sessions’ rows');
      await go(b);
      check('the other session shows nothing on the side', !strip() && !aside());
      await go(a);
      await until(strip, 5000, 'the side conversation above the box');
      await until(function () { return /arrive in the order/.test(strip().textContent); }, 5000, 'its answer');
      check('coming to the session shows the one that answered, closed as it is', /Why a queue\?/.test(strip().textContent));
      check('and the box is its', aside());

      acts('Close').click();
      await until(function () { return !strip(); }, 3000, 'the side conversation put away');
      check('put away, the box is the session’s', !aside());
      await go(b);
      await go(a);
      check('and it doesn’t come back up in this window', !strip() && !aside());

      // Another answer, while the session shows.
      window.__answer = { id: 'closed-1', at: t + 1000 };
      await window.__pushSides();
      await wait(200);
      check('one that comes in while the session shows takes nothing from the box', !strip() && !aside());
      window.dispatchEvent(new Event('focus'));
      await until(strip, 3000, 'the side conversation as the window comes forward');
      check('the window coming forward with it showing brings it up', /Why a queue\?/.test(strip().textContent) && aside());

      // Marked seen: the board has no answer for it any more.
      window.__answer = null;
      await window.__pushSides();
      await until(function () { return !strip(); }, 3000, 'the side conversation gone once it’s seen');
      check('once it’s seen, one that came up by itself goes, and the box is the session’s', !aside());
      check('nothing of it stays in the view', localStorage.getItem('codealong.sideview.' + key) === null);

      // Seen while the window shows another session: it hears nothing until it's back.
      window.__answer = { id: 'closed-1', at: t + 1500 };
      await window.__pushSides();
      await wait(200);
      window.dispatchEvent(new Event('focus'));
      await until(strip, 3000, 'the side conversation up again');
      await go(b);
      window.__answer = null;
      await window.__pushSides();
      await go(a);
      check('seen while it showed another session, it’s gone by the time you’re back', !strip() && !aside());

      // One you picked is yours to put away, seen or not.
      var line = await until(function () { return all('.ca-sessinsp-kin.is-side').filter(function (el) { return /Why a queue\?/.test(el.textContent); })[0]; }, 5000, 'its line in the header');
      line.click();
      await until(strip, 3000, 'the side conversation picked from the header');
      await go(b);
      await go(a);
      check('one you picked stays, seen or not', !!strip() && aside());
      acts('Close').click();
      await until(function () { return !strip(); }, 3000, 'it put away');

      // An open one: the same, and Close is the engine's.
      var open = { id: 'open-1', key: key, turns: [{ question: 'And the lock file?', answer: 'Gone.', done: true, at: t + 2000 }], running: false, ended: false, startedAt: t + 2000 };
      window.__sides = {};
      window.__sides[key] = open;
      window.__emit('side', open);
      await until(strip, 3000, 'the open one above the box');
      await go(b);
      localStorage.setItem('codealong.sideview.' + key, JSON.stringify({ kind: 'list' }));
      window.dispatchEvent(new StorageEvent('storage', { key: 'codealong.sideview.' + key }));
      window.__answer = { id: 'open-1', open: true, at: t + 2000 };
      await window.__pushSides();
      await go(a);
      await until(function () { return strip() && /And the lock file\?/.test(strip().textContent); }, 3000, 'the open one, not the list');
      check('coming to the session shows the open one that answered, whatever showed there', /Gone\./.test(strip().textContent) && aside());
      done();
    } catch (e) {
      stopped(String(e));
    }
  })();
})();
