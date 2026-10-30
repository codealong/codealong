// Checks that the waiting window, working through what waits on you, comes to a session a side
// conversation's answer brought there with that side conversation above its box, as every window
// that shows a session does (SessionReply.tsx `Reply`, side.ts `useSideAnswer`): one closed
// while it answered, read there, the box going to it; and that ⌘E marks the session seen.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/side-answer-waiting-check.js
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && !l.device && !s.visitor && !s.helper && !s.headless && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to wait on you'); return; }
  var key = picked.key;
  localStorage.removeItem('codealong.sideview.' + key);
  var t = Date.now() - 60000;
  window.__past = [{ id: 'closed-1', key: key, title: '', startedAt: t, lastAt: t, turns: [{ question: 'Why a queue?', answer: 'Messages arrive in the order you sent them.', done: true, at: t }] }];
  // Moved to the top, so it's the one the window opens on whatever else waits.
  var answer = { status: 'unread', statusSince: t, excerpt: 'Answered on the side · Why a queue?', sideAnswer: { id: 'closed-1', at: t }, raised: { at: Date.now() }, snoozedUntil: null, live: true, stalledSince: null, asks: false, background: undefined };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, answer); }); });
      return b;
    });
  };
  var strip = function () { return q('.ca-sidestrip'); };

  (async function () {
    try {
      await until(function () { return strip() && /in the order you sent/.test(strip().textContent); }, 20000, 'the side conversation above the box of the session it brought here');
      check('the side conversation shows, read as it was closed', /Why a queue\?/.test(strip().textContent));
      check('and the box is its', !!q('.ca-composer.is-aside'));
      var seen = q('.ca-hotkey[title*="seen" i]');
      check('the strip offers to mark it seen', !!seen && !seen.disabled);
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'e', code: 'KeyE', metaKey: true, bubbles: true, cancelable: true }));
      await until(function () { return calls('mark_seen').length > 0; }, 3000, 'the session marked seen');
      check('and ⌘E marks it seen, as for a turn that ended', calls('mark_seen')[0][1].keys[0] === key);
      done();
    } catch (e) {
      stopped(String(e));
    }
  })();
})();
