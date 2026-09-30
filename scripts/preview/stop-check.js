// Checks Stop in the reply box (SessionReply.tsx), in the bar's peek: a session whose turn can be
// stopped shows Stop in the box's corner while the box is empty, Send once something's typed, and
// says Escape stops it; Escape in the box asks for session_cancel once and doesn't close the peek,
// the button says Stopping and the box says what happened; another Escape meanwhile asks nothing
// more and closes the peek as usual, as ⌘. asks nothing more; and the button goes once the
// session's turn has ended.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/stop-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };

  // This runs before the app's script: pick a session to peek at, and have it working with a
  // turn Codealong can stop.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden) picked = s; }); });
  if (!picked) { say('done: no session to peek at'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  var turn = { status: 'working', live: true, stoppable: true, stalledSince: null, asks: false };
  var withTurn = function (b, t) {
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, t); }); });
    return b;
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    return cmd === 'get_board' ? got.then(function (b) { return withTurn(b, turn); }) : got;
  };

  var stopButton = function () { return document.querySelector('.ca-composer-send.is-stop'); };
  var sendButton = function () { return document.querySelector('.ca-composer-send:not(.is-stop)'); };
  var box = function () { return document.querySelector('.ca-composer textarea'); };
  var noteText = function () { var n = document.querySelector('.ca-composer-note'); return n ? n.textContent : ''; };
  var cancels = function () { return window.__calls.filter(function (c) { return c[0] === 'session_cancel'; }); };
  var hides = function () { return window.__calls.filter(function (c) { return c[0] === 'peek_hide'; }).length; };
  var cmdDot = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: '.', metaKey: true, bubbles: true, cancelable: true })); };
  var escape = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };

  (async function () {
    try {
      await until(stopButton, 20000);
      check('shows Stop in the box', stopButton().getAttribute('aria-label') === 'Stop' && !sendButton());
      check('says Escape stops it', noteText().indexOf('Esc stops') === 0, noteText());
      type(box(), 'and then');
      await wait(50);
      check('turns to Send once something is typed', !stopButton() && sendButton() && !sendButton().disabled);
      type(box(), '');
      await wait(50);
      check('and back to Stop when emptied', !!stopButton());
      box().focus();
      var hidden = hides();
      escape();
      await wait(50);
      var asked = cancels();
      check('Escape in the box stops it', asked.length === 1 && asked[0][1].key === key, JSON.stringify(asked.map(function (c) { return c[1]; })));
      check('and leaves the peek open', hides() === hidden, String(hides() - hidden));
      check('says Stopping', stopButton() && stopButton().getAttribute('aria-label') === 'Stopping' && stopButton().disabled);
      await until(function () { return noteText().indexOf('Stopped') === 0; }, 5000);
      check('says what happened', true);
      escape();
      cmdDot();
      await wait(50);
      check('asks once while stopping', cancels().length === 1, String(cancels().length));
      check('Escape then closes the peek as usual', hides() === hidden + 1, String(hides() - hidden));
      // The turn ends: the next board has it idle.
      turn = { status: 'idle', live: true, stoppable: false };
      var next = await window.__TAURI_INTERNALS__.invoke('get_board');
      window.__emit('board', next);
      await until(function () { return !stopButton(); }, 5000);
      check('goes once the turn has ended', true);
    } catch (e) {
      check('ran', false, String(e) + ' note: ' + noteText());
    }
    say('done: ' + out.join('; '));
  })();
})();
