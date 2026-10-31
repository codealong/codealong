// Checks Stop in the reply box (SessionReply.tsx), in the bar's peek: a session whose turn can be
// stopped shows Stop in the box's corner while the box is empty, Send once something's typed,
// and its placeholder says Escape twice and ⌘. stop it; one Escape in the box stops nothing,
// leaves the peek open and says in amber that another stops it, which typing or a couple of
// seconds take back; a second asks for session_cancel once, the button says Stopping and the box
// says what happened; Escape then closes the peek as usual, and ⌘. asks nothing more; the button
// goes once the session's turn has ended, and Escape leaves; and on the next turn the button
// stops it.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/stop-check.js
(function () {
  // This runs before the app's script: pick a session to peek at, and have it working with a
  // turn Codealong can stop.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden) picked = s; }); });
  if (!picked) { stopped('no session to peek at'); return; }
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
  var note = function () { return document.querySelector('.ca-composer-note'); };
  var noteText = function () { var n = note(); return n ? n.textContent : ''; };
  var again = function () { return noteText() === 'Esc again stops its turn' && note().classList.contains('is-attn'); };
  var cancels = function () { return window.__calls.filter(function (c) { return c[0] === 'session_cancel'; }); };
  var hides = function () { return window.__calls.filter(function (c) { return c[0] === 'peek_hide'; }).length; };
  var cmdDot = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: '.', metaKey: true, bubbles: true, cancelable: true })); };
  var escape = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); };

  (async function () {
    try {
      await until(stopButton, 20000);
      check('shows Stop in the box', stopButton().getAttribute('aria-label') === 'Stop' && !sendButton());
      check('its placeholder says Escape twice and ⌘. stop it', /esc twice or ⌘\. stops its turn$/.test(box().getAttribute('aria-placeholder')), box().getAttribute('aria-placeholder'));
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
      check('one Escape in the box stops nothing', cancels().length === 0, JSON.stringify(cancels().map(function (c) { return c[1]; })));
      check('and leaves the peek open', hides() === hidden, String(hides() - hidden));
      check('says in amber that another stops it', again(), noteText());
      type(box(), 'x');
      type(box(), '');
      await wait(50);
      check('typing takes that back', !again(), noteText());
      escape();
      await wait(50);
      check('so the next Escape stops nothing', cancels().length === 0 && again(), String(cancels().length) + ' ' + noteText());
      await wait(2300);
      check('nor does one a couple of seconds later', !again(), noteText());
      escape();
      await wait(50);
      check('which says it again', cancels().length === 0 && again(), String(cancels().length) + ' ' + noteText());
      escape();
      await wait(50);
      var asked = cancels();
      check('a second Escape stops it', asked.length === 1 && asked[0][1].key === key, JSON.stringify(asked.map(function (c) { return c[1]; })));
      check('says Stopping', stopButton() && stopButton().getAttribute('aria-label') === 'Stopping' && stopButton().disabled);
      check('with the peek still open', hides() === hidden, String(hides() - hidden));
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
      box().focus();
      escape();
      await wait(50);
      check('and Escape leaves at once', hides() === hidden + 2 && cancels().length === 1, String(hides() - hidden));
      // The next turn: the button stops it.
      turn = { status: 'working', live: true, stoppable: true, stalledSince: null, asks: false };
      window.__emit('board', await window.__TAURI_INTERNALS__.invoke('get_board'));
      await until(function () { return stopButton() && !stopButton().disabled; }, 5000);
      stopButton().click();
      await wait(50);
      check('the button stops the next', cancels().length === 2, String(cancels().length));
    } catch (e) {
      check('ran', false, String(e) + ' note: ' + noteText());
    }
    done();
  })();
})();
