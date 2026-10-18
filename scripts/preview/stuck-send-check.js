// Checks that a stuck session takes a message (SessionReply.tsx `takesReply`, lanes.ts
// `waitsOnYou`), in the bar's peek: one stuck mid-turn on a command shows no box telling you to
// open it, its placeholder says it takes the message once the step it's stuck on ends, and ⏎
// sends what's typed while the thread still says what it's stuck on; one stuck on a command its
// turn left running in the background (a sign-in waiting on a device code) sends as any prompt.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/stuck-send-check.js
(function () {
  // This runs before the app's script: pick a Claude Code session to peek at, and have it stuck.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude' && s.surface === 'cli' && !s.hosted) picked = s; }); });
  if (!picked) { stopped('no Claude Code terminal session to peek at'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  var since = Date.now() - 40 * 60000;
  var stuck = { status: 'needs', statusSince: since, live: true, pid: picked.pid || 4242, stoppable: true, steerable: false, stalledSince: since, background: undefined, asks: false, excerpt: 'Stuck on `northwind login --device` for {for:' + since + '}' };
  window.__madeSessions = {};
  window.__madeSessions[key] = Object.assign({}, picked, stuck);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, stuck); }); });
      return b;
    });
  };

  var box = function () { return document.querySelector('.ca-peek .ca-composer textarea'); };
  var sendButton = function () { return document.querySelector('.ca-peek .ca-composer-send[aria-label="Send"]'); };
  var openBox = function () { return document.querySelector('.ca-peek .ca-composer.is-waiting'); };
  var stuckLine = function () { var n = document.querySelector('.ca-peek .ca-thread-tool.is-asks'); return n ? n.textContent : ''; };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var sends = function () { return calls('send_to_session'); };

  (async function () {
    try {
      await until(box, 20000);
      await until(function () { return /: it takes this once the step it’s stuck on ends( · |$)/.test(box().getAttribute('aria-placeholder') || ''); }, 5000).catch(function () {});
      check('a session stuck mid-turn shows no box telling you to open it', !openBox(), openBox() ? openBox().textContent : '');
      check('its placeholder says it takes it once that step ends', /: it takes this once the step it’s stuck on ends( · |$)/.test(box().getAttribute('aria-placeholder') || ''), box().getAttribute('aria-placeholder'));
      check('the thread still says what it’s stuck on', /^Stuck on northwind login --device for /.test(stuckLine()), stuckLine());
      type(box(), 'Just logged in');
      await wait(50);
      check('Send is there once something is typed', !!sendButton() && !sendButton().disabled, sendButton() ? 'disabled' : 'no Send');
      enter();
      await until(function () { return sends().length === 1; }, 5000).catch(function () {});
      var sent = sends();
      check('⏎ sends it', sent.length === 1 && sent[0][1].key === key && sent[0][1].text === 'Just logged in', JSON.stringify(sent.map(function (c) { return c[1]; })));

      // Its turn ended on the sign-in, left running in the background.
      stuck = Object.assign({}, stuck, { background: 'northwind login --device', stoppable: false });
      window.__madeSessions[key] = Object.assign({}, picked, stuck);
      window.__emit('board', await window.__TAURI_INTERNALS__.invoke('get_board'));
      await until(function () { return !/stuck on/.test(box().getAttribute('aria-placeholder') || ''); }, 5000).catch(function () {});
      check('one stuck on a command in the background shows no box telling you to open it either', !openBox());
      check('and its placeholder is a plain send', !/stuck on|waiting/.test(box().getAttribute('aria-placeholder') || ''), box().getAttribute('aria-placeholder'));
      type(box(), 'Approved it');
      await wait(50);
      enter();
      await until(function () { return sends().length === 2; }, 5000).catch(function () {});
      check('and ⏎ sends to it', sends().length === 2 && sends()[1][1].text === 'Approved it', String(sends().length));
    } catch (e) {
      check('ran', false, String(e));
    }
    done();
  })();
})();
