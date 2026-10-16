// Checks Send now in a session's reply box (SessionReply.tsx), in the floating bar's peek: while
// its turn runs and the engine says it can take a message at once (`steerable`), the bolt beside
// Send and ⌃⏎ (and ⌃⌥⏎, as it always has) send with `now`, ⏎ sends as usual, and the bolt's tooltip says the key, which the
// peek, with no strip to say keys on, doesn't say under the box; with nothing typed there's no
// bolt. sessions-says-check.js checks the strip saying it.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/send-now-check.js
(function () {
  // This runs before the app's script: pick a session that doesn't wait on you, with no draft,
  // and make it one whose turn runs and can take a message at once.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  var pickedLane = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !s.hosted && !l.hidden) { picked = s; pickedLane = l; } }); });
  if (!picked) { stopped('no session to peek at'); return; }
  var key = picked.key;
  var running = function (s) { return Object.assign({}, s, { status: 'working', live: true, stoppable: true, steerable: true }); };
  picked = running(picked);
  // The stub's guess at how a send reaches it goes by this one, not board.json's.
  window.__madeSessions = {};
  window.__madeSessions[key] = picked;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);

  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_board') {
      return real.apply(this, arguments).then(function (b) {
        b.lanes.forEach(function (l) { l.sessions = l.sessions.map(function (s) { return s.key === key ? running(s) : s; }); });
        return b;
      });
    }
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ message: args.now ? 'Sent now.' : 'Sent.', opened: false });
    }
    return real.apply(this, arguments);
  };

  var box = function () { return document.querySelector('.ca-peek .ca-composer textarea'); };
  var note = function () { var n = document.querySelector('.ca-peek .ca-composer-note'); return n ? n.textContent : ''; };
  var bolt = function () { return document.querySelector('.ca-peek .ca-composer-send.is-now'); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var enter = function (el, ctrl, alt) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: !!ctrl, altKey: !!alt, bubbles: true, cancelable: true })); };
  var sends = function () { return window.__calls.filter(function (c) { return c[0] === 'send_to_session'; }); };
  var last = function () { var s = sends(); return s[s.length - 1][1]; };
  var opens = 1;
  var show = function () {
    opens += 1;
    window.__emit('peek-open', { id: opens, lane: pickedLane, session: picked, terminal: board.apps.terminal, theme: board.theme, room: 608, below: true, focus: false });
    return until(function () { return !!box(); }, 5000);
  };
  var hide = function () { window.__emit('peek-hidden', null); return until(function () { return !box(); }, 5000); };

  say('send now check: waiting for the peek');
  until(function () { return !!box(); }, 20000)
    .then(function () { return until(function () { return !!document.querySelector('.ca-peek .ca-composer-send.is-stop'); }, 5000).catch(function () {}); })
    .then(function () {
      check('no bolt with nothing typed, where Stop is', !bolt() && !!document.querySelector('.ca-peek .ca-composer-send.is-stop'));
      check('nothing under the box says the key', note() === '', note());
      type(box(), 'Also check the docs');
      return until(function () { return !!bolt(); }, 2000).catch(function () {});
    })
    .then(function () {
      check('the bolt shows beside Send once something is typed', !!bolt() && !!document.querySelector('.ca-peek .ca-composer-send[aria-label="Send"]'));
      check('its tooltip says the key', !!bolt() && bolt().title.indexOf('⌃⏎') >= 0, bolt() && bolt().title);
      bolt().click();
      return until(function () { return sends().length === 1; }, 5000);
    })
    .then(function () {
      check('the bolt sends it now', last().text === 'Also check the docs' && last().now === true, JSON.stringify(last()));
      return hide().then(show);
    })
    .then(function () {
      type(box(), 'And the tests');
      enter(box(), true);
      return until(function () { return sends().length === 2; }, 5000);
    })
    .then(function () {
      check('⌃⏎ sends it now', last().text === 'And the tests' && last().now === true, JSON.stringify(last()));
      return hide().then(show);
    })
    .then(function () {
      type(box(), 'Then the build');
      enter(box(), true, true);
      return until(function () { return sends().length === 3; }, 5000);
    })
    .then(function () {
      check('⌃⌥⏎ sends it now too', last().text === 'Then the build' && last().now === true, JSON.stringify(last()));
      return hide().then(show);
    })
    .then(function () {
      type(box(), 'When you’re done');
      enter(box());
      return until(function () { return sends().length === 4; }, 5000);
    })
    .then(function () {
      check('⏎ sends it as usual', last().text === 'When you’re done' && last().now === false, JSON.stringify(last()));
    })
    .then(done)
    .catch(stopped);
})();
