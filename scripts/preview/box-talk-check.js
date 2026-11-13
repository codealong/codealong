// Checks a session's box's microphone (SessionReply.tsx `TalkButton`, talk.ts `useGoing`), in the
// floating bar's peek: with voice on the box has one beside Send, whose tooltip names the key held
// to talk; a click gives the box the keyboard and begins a press, and the microphone shows it
// listening once the engine tells the window of it (dictation.rs `told`), bar or no bar; ⏎ ends
// the press rather than sending what the box held, and while the last words settle does nothing;
// what was said is typed at the caret, and the microphone goes quiet once the press is over; a
// click while it listens ends it, with the keyboard given back to the box first, so a keyboard
// user's Space on it still types at the caret; a press begun while another field had the
// keyboard isn't the box's, nor one to your orchestrator; the peek put away drops a press its
// microphone began, but not one begun by the key held; and what's heard is said at the window's
// foot only when the engine says the window stands in for the bar.
//   PREVIEW_VOICE=1 PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/box-talk-check.js
(function () {
  // This runs before the app's script: the peek shows a session that doesn't wait on you.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  var pickedLane = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !l.hidden && !s.visitor) { picked = s; pickedLane = l; } }); });
  if (!picked) { stopped('no session to peek at'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);
  var keys = board.talkShortcut ? board.talkShortcut : '';

  var box = function () { return q('.ca-peek .ca-composer textarea'); };
  var mic = function () { return q('.ca-peek .ca-composer-mic'); };
  var on = function () { return !!mic() && mic().getAttribute('aria-pressed') === 'true'; };
  var setText = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var id = 0;
  var talk = function (over) { window.__emit('talk', Object.assign({ id: id, phase: 'listening', to: 'looking', text: '', level: 0.3 }, over)); };
  var talks = function () { return sent('talk').length; };
  var opens = 1;
  var show = function () {
    opens += 1;
    window.__emit('peek-open', { id: opens, lane: pickedLane, session: picked, terminal: board.apps.terminal, theme: board.theme, room: 608, below: true, focus: false });
    return until(function () { return !!box() && !!mic(); }, 5000, 'the peek shown again');
  };
  var hide = function () { window.__emit('peek-hidden', null); return until(function () { return !box(); }, 5000, 'the peek put away'); };

  say('box talk check: waiting for the peek');
  until(function () { return box() && mic(); }, 20000, 'the peek’s box and its microphone')
    .then(function () {
      check('with voice on, the box has a microphone beside Send', mic().getAttribute('aria-pressed') === 'false' && mic().getAttribute('aria-label') === 'Talk' && !!mic().nextElementSibling, mic().outerHTML.slice(0, 200));
      check('its tooltip says what a click does', /^Talk, and what you say is typed here: click, and click again when you’re done/.test(mic().title), mic().title);
      if (keys) check('and names the key held to talk', keys !== 'Option+Cmd+[' ? /, or hold \S+\.$/.test(mic().title) : /, or hold ⌥⌘\[\.$/.test(mic().title), mic().title + ' for ' + keys);
      else note('the board names no key held to talk: the tooltip names none');
      check('the box doesn’t have the keyboard yet', document.activeElement !== box());
      setText(box(), 'Then:');
      mic().click();
      return until(function () { return talks() === 1; }, 3000, 'the press begun');
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
      return until(function () { return talks() === 2; }, 3000, 'the press ended');
    })
    .then(function () {
      check('⏎ ends the press', last('talk').on === false && last('talk').drop == null, JSON.stringify(last('talk')));
      check('and sends nothing', calls('send_to_session').length === 0, calls('send_to_session').length + ' sent');
      talk({ phase: 'settling', text: 'Fix the login' });
      return wait(100);
    })
    .then(function () {
      check('it still shows listening while the last words settle', on());
      key(box(), { key: 'Enter' });
      return wait(150);
    })
    .then(function () {
      check('and ⏎ meanwhile neither sends what the box held nor asks again', calls('send_to_session').length === 0 && talks() === 2, calls('send_to_session').length + ' sent, ' + talks() + ' asked');
      window.__emit('talked', { id: id, text: 'Fix the login' });
      talk({ phase: 'typed', text: 'Fix the login' });
      return until(function () { return !on(); }, 3000, 'the microphone quiet again');
    })
    .then(function () {
      check('what was said is typed at the caret', box().value === 'Then: Fix the login', box().value);
      check('the microphone is quiet once it’s typed', mic().getAttribute('aria-label') === 'Talk' && !mic().querySelector('.ca-spoken-level'));
      mic().click();
      return until(function () { return talks() === 3; }, 3000, 'another press begun');
    })
    .then(function () {
      id = 2;
      talk({});
      return until(on, 3000, 'the microphone listening again');
    })
    .then(function () {
      // As a keyboard user stops it: Space on the microphone, which has the keyboard.
      box().setSelectionRange(5, 5);
      mic().focus();
      mic().click();
      return until(function () { return talks() === 4; }, 3000, 'the click that ends it');
    })
    .then(function () {
      check('a click while it listens ends the press', last('talk').on === false && last('talk').drop == null, JSON.stringify(last('talk')));
      check('with the keyboard back in the box first, its caret where it was', document.activeElement === box() && box().selectionStart === 5, (document.activeElement && document.activeElement.className) + ' at ' + box().selectionStart);
      window.__emit('talked', { id: id, text: 'and' });
      talk({ phase: 'typed', text: 'and' });
      return until(function () { return !on(); }, 3000, 'the microphone quiet');
    })
    .then(function () {
      check('so what was said is typed at that caret', box().value === 'Then: and Fix the login', box().value);
      // A press to your orchestrator, begun while the box has the keyboard: its words go there.
      id = 3;
      talk({ to: 'orchestrator' });
      return wait(150);
    })
    .then(function () {
      check('a press to your orchestrator isn’t the box’s', !on());
      talk({ to: 'orchestrator', phase: 'sent' });
      // A press begun while the keyboard is elsewhere: its words go there, not into the box.
      box().blur();
      id = 4;
      talk({});
      return wait(150);
    })
    .then(function () {
      check('a press begun while the box hadn’t the keyboard isn’t the box’s', !on());
      talk({ phase: 'nothing' });
      mic().click();
      return until(function () { return talks() === 5; }, 3000, 'a press begun to be put away');
    })
    .then(function () {
      id = 5;
      talk({});
      return until(on, 3000, 'the microphone listening');
    })
    .then(hide)
    .then(function () { return until(function () { return talks() === 6; }, 3000, 'the press dropped'); })
    .then(function () {
      check('the peek put away drops the press its microphone began, by its id', last('talk').on === false && last('talk').drop === 5, JSON.stringify(last('talk')));
      talk({ phase: 'nothing' });
      return show();
    })
    .then(function () {
      // Begun by the key held while the box has the keyboard: the key's coming up ends it.
      box().focus();
      id = 6;
      talk({});
      return until(on, 3000, 'the microphone listening for the key');
    })
    .then(function () {
      check('a press begun by the key held shows on the box’s microphone too', on());
      return hide();
    })
    .then(function () { return wait(150); })
    .then(function () {
      check('and the peek put away doesn’t drop it', talks() === 6, JSON.stringify(last('talk')));
      talk({ phase: 'nothing' });
      id = 7;
      talk({ standsIn: true, text: 'Check the build' });
      return until(function () { return !!q('.ca-talking'); }, 3000, 'the line at the window’s foot');
    })
    .then(function () {
      check('with no bar to show it, the window says what’s heard at its foot', /Check the build/.test(q('.ca-talking').textContent), q('.ca-talking').textContent);
      talk({ phase: 'nothing', standsIn: true });
    })
    .then(done, stopped);
})();
