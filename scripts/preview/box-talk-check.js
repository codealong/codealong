// Checks a session's box's microphone (SessionReply.tsx `TalkButton`, talk.ts `useGoing`), in the
// floating bar's peek: with voice on the box has one beside Send, whose tooltip names the key held
// to talk; a click gives the box the keyboard and begins a press, and the microphone shows it
// listening once the engine tells the window of it (dictation.rs `told`), bar or no bar; ⏎ ends
// the press rather than sending what the box held; what was said is typed at the caret, and the
// microphone goes quiet once the press is over; a click while it listens ends it; a press begun
// while another field had the keyboard isn't the box's; and what's heard is said at the window's
// foot only when the engine says the window stands in for the bar.
//   PREVIEW_VOICE=1 PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/box-talk-check.js
(function () {
  // This runs before the app's script: the peek shows a session that doesn't wait on you.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !l.hidden && !s.visitor) picked = s; }); });
  if (!picked) { stopped('no session to peek at'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);
  var keys = board.talkShortcut ? board.talkShortcut : '';

  var box = function () { return q('.ca-peek .ca-composer textarea'); };
  var mic = function () { return q('.ca-peek .ca-composer-mic'); };
  var on = function () { return mic() && mic().getAttribute('aria-pressed') === 'true'; };
  var id = 0;
  var talk = function (over) { window.__emit('talk', Object.assign({ id: id, phase: 'listening', to: 'looking', text: '', level: 0.3 }, over)); };

  say('box talk check: waiting for the peek');
  until(function () { return box() && mic(); }, 20000, 'the peek’s box and its microphone')
    .then(function () {
      check('with voice on, the box has a microphone beside Send', mic().getAttribute('aria-pressed') === 'false' && mic().getAttribute('aria-label') === 'Talk' && !!mic().nextElementSibling, mic().outerHTML.slice(0, 200));
      check('its tooltip says what a click does', /^Talk, and what you say is typed here: click, and click again when you’re done/.test(mic().title), mic().title);
      if (keys) check('and names the key held to talk', keys !== 'Option+Cmd+[' ? /, or hold \S+\.$/.test(mic().title) : /, or hold ⌥⌘\[\.$/.test(mic().title), mic().title + ' for ' + keys);
      else note('the board names no key held to talk: the tooltip names none');
      check('the box doesn’t have the keyboard yet', document.activeElement !== box());
      mic().click();
      return until(function () { return sent('talk').length === 1; }, 3000, 'the press begun');
    })
    .then(function () {
      check('a click begins a press', last('talk').on === true && last('talk').drop == null, JSON.stringify(last('talk')));
      check('with the box given the keyboard, where the words go', document.activeElement === box());
      id = 1;
      talk({});
      return until(on, 3000, 'the microphone listening');
    })
    .then(function () {
      check('the microphone shows it listening, and how loud', mic().classList.contains('is-on') && !!mic().querySelector('.ca-spoken-level') && mic().getAttribute('aria-label') === 'Stop listening', mic().innerHTML.slice(0, 200));
      check('its tooltip says how it ends', /^Listening: click again, or (⏎|Enter)/.test(mic().title), mic().title);
      check('with a bar to show what’s heard, the window says nothing of it at its foot', !q('.ca-talking'));
      key(box(), { key: 'Enter' });
      return until(function () { return sent('talk').length === 2; }, 3000, 'the press ended');
    })
    .then(function () {
      check('⏎ ends the press', last('talk').on === false, JSON.stringify(last('talk')));
      check('and sends nothing', calls('send_to_session').length === 0, calls('send_to_session').length + ' sent');
      talk({ phase: 'settling', text: 'Fix the login' });
      return wait(100);
    })
    .then(function () {
      check('it still shows listening while the last words settle', on());
      window.__emit('talked', { id: id, text: 'Fix the login' });
      talk({ phase: 'typed', text: 'Fix the login' });
      return until(function () { return !on(); }, 3000, 'the microphone quiet again');
    })
    .then(function () {
      check('what was said is typed at the caret', box().value === 'Fix the login', box().value);
      check('the microphone is quiet once it’s typed', mic().getAttribute('aria-label') === 'Talk' && !mic().querySelector('.ca-spoken-level'));
      mic().click();
      return until(function () { return sent('talk').length === 3; }, 3000, 'another press begun');
    })
    .then(function () {
      id = 2;
      talk({});
      return until(on, 3000, 'the microphone listening again');
    })
    .then(function () {
      mic().click();
      return until(function () { return sent('talk').length === 4; }, 3000, 'the click that ends it');
    })
    .then(function () {
      check('a click while it listens ends the press', last('talk').on === false, JSON.stringify(last('talk')));
      talk({ phase: 'nothing' });
      return until(function () { return !on(); }, 3000, 'the microphone quiet');
    })
    .then(function () {
      // A press begun while the keyboard is elsewhere: its words go there, not into the box.
      box().blur();
      id = 3;
      talk({});
      return wait(150);
    })
    .then(function () {
      check('a press begun while the box hadn’t the keyboard isn’t the box’s', !on());
      talk({ phase: 'nothing' });
      id = 4;
      talk({ standsIn: true, text: 'Check the build' });
      return until(function () { return !!q('.ca-talking'); }, 3000, 'the line at the window’s foot');
    })
    .then(function () {
      check('with no bar to show it, the window says what’s heard at its foot', /Check the build/.test(q('.ca-talking').textContent), q('.ca-talking').textContent);
      talk({ phase: 'nothing', standsIn: true });
    })
    .then(done, stopped);
})();
