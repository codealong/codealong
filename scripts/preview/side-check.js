// Checks /side in the reply box (SessionReply.tsx, slash.ts, SideStrip.tsx), in the bar's peek:
// typing `/s` says what /side does; ⏎ on `/btw <question>` asks a copy of the session, sends the
// session nothing, clears the box and shows the question above it; the answer shows as the engine
// tells of it (the tool the copy runs, then its words); a second question joins the first; Close
// ends it; a question that can't be asked is back in the box with why; and /side alone asks
// nothing.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/side-check.js
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

  // A Claude Code session to peek at, idle and taking messages.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { say('done: no Claude Code session to peek at'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  var calm = { status: 'idle', live: true, stoppable: false, stalledSince: null, asks: false };
  var failNext = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'side_ask' && failNext) {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.reject('Its folder is gone, so a copy of it has nowhere to run.');
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, calm); }); });
      return b;
    });
  };

  var box = function () { return document.querySelector('.ca-composer textarea'); };
  var button = function () { return document.querySelector('.ca-composer-send'); };
  var noteText = function () { var n = document.querySelector('.ca-composer-note'); return n ? n.textContent : ''; };
  var strip = function () { return document.querySelector('.ca-side'); };
  var turns = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-side-turn'), function (t) { return t.textContent; }); };
  var calls = function (name) { return window.__calls.filter(function (c) { return c[0] === name; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // What the engine would say, which the stub's next answer goes on from too.
  var tell = function (side) {
    side = Object.assign({ key: key, running: true, ended: false }, side);
    window.__sides = window.__sides || {};
    window.__sides[key] = side;
    window.__emit('side', side);
  };

  (async function () {
    try {
      await until(box, 20000);
      check('no side conversation shows until one is asked', !strip());
      type(box(), '/s');
      await wait(50);
      check('a / and the start of its name says what /side does', /^\/side <question>: /.test(noteText()), noteText());
      type(box(), '/btw why a queue?');
      await wait(50);
      check('/btw is the same command', button().getAttribute('aria-label') === 'Run /side' && !button().disabled, button().getAttribute('aria-label'));
      var sends = calls('send_to_session').length;
      var hides = calls('peek_hide').length;
      enter();
      await until(strip, 5000);
      var asks = calls('side_ask');
      check('⏎ asks a copy of the session', asks.length === 1 && asks[0][1].key === key && asks[0][1].text === 'why a queue?', JSON.stringify(asks.map(function (c) { return c[1]; })));
      check('and sends the session nothing', calls('send_to_session').length === sends);
      check('clears the box', box().value === '', box().value);
      check('leaves the peek open', calls('peek_hide').length === hides);
      check('shows the question above the box', turns().length === 1 && turns()[0].indexOf('why a queue?') === 0 && turns()[0].indexOf('Reading the conversation') > 0, turns().join(' / '));
      check('the strip sits right above the box', strip().nextElementSibling === document.querySelector('.ca-composer:last-child'));

      tell({ turns: [{ question: 'why a queue?', answer: '', tool: 'Bash · `git log -3`', done: false }] });
      await wait(50);
      check('shows the tool the copy runs', turns()[0].indexOf('Bash · git log -3') > 0 && turns()[0].indexOf('Reading') < 0, turns()[0]);
      tell({ turns: [{ question: 'why a queue?', answer: 'Because sends must keep their **order**.', done: false }] });
      await wait(50);
      check('then its answer as it comes', turns()[0].indexOf('Because sends must keep their order.') > 0 && !!strip().querySelector('.ca-side-a strong'), turns()[0]);
      tell({ running: false, turns: [{ question: 'why a queue?', answer: 'Because sends must keep their **order**.', done: true }] });
      await wait(50);
      check('and nothing more once it has answered', !strip().querySelector('.ca-side-doing'));

      type(box(), '/side and without one?');
      await wait(50);
      enter();
      await until(function () { return turns().length === 2; }, 5000);
      check('a second question joins the first', calls('side_ask')[1][1].text === 'and without one?' && turns()[0].indexOf('order') > 0, turns().join(' / '));
      tell({ turns: [{ question: 'why a queue?', answer: 'Because.', done: true }, { question: 'and without one?', answer: '', done: true, error: 'The copy stopped: usage limit reached' }], running: false, ended: true });
      await wait(50);
      check('a copy that stops says why', turns()[1].indexOf('The copy stopped: usage limit reached') > 0 && strip().textContent.indexOf('Its copy has ended') > 0, strip().textContent);

      type(box(), 'while it is open, plain text still goes to the session');
      await wait(50);
      enter();
      await until(function () { return calls('send_to_session').length > sends; }, 5000);
      check('plain text still goes to the session', calls('side_ask').length === 2);

      var closeButton = strip().querySelector('.ca-side-head button');
      closeButton.click();
      await until(function () { return !strip(); }, 5000);
      check('Close ends it', calls('side_close').length === 1 && calls('side_close')[0][1].key === key);

      failNext = true;
      type(box(), '/side where does it run?');
      await wait(50);
      enter();
      await until(function () { return noteText().indexOf('Its folder is gone') >= 0; }, 5000);
      check('a question that can’t be asked says why', !strip());
      check('and is back in the box', box().value === '/side where does it run?', box().value);
      failNext = false;

      var before = calls('side_ask').length;
      type(box(), '/side');
      await wait(50);
      enter();
      await wait(100);
      check('/side alone asks nothing', calls('side_ask').length === before && noteText().indexOf('Ask it something') === 0, noteText());
    } catch (e) {
      out.push('error: ' + e.message);
    }
    say('done: ' + out.join(' | '));
  })();
})();
