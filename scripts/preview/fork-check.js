// Checks /fork in the reply box (SessionReply.tsx, slash.ts), in the bar's peek: typing a `/` and
// the start of the name says what /fork does, and the send button shows a fork; ⏎ on `/fork <prompt>` asks for a background fork
// with the prompt, clears the box, leaves the peek open and says what happened; a fork that fails
// puts what was typed back with why; and any other `/…` is sent to the session as typed.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/fork-check.js
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
    if (cmd === 'fork_session') {
      window.__calls.push([cmd, arguments[1]]);
      if (failNext) return Promise.reject('Workspace not trusted.');
      return Promise.resolve({ key: 'claude:claude:f0000000-0000-4000-8000-000000000000', message: 'Forked in the background: the copy of “x” is on it.' });
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
  var calls = function (name) { return window.__calls.filter(function (c) { return c[0] === name; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };

  (async function () {
    try {
      await until(box, 20000);
      type(box(), '/fo');
      await wait(50);
      check('a / and the start of a name says what /fork does', /^\/fork \[prompt\]: /.test(noteText()), noteText());
      type(box(), '/fork try it with a queue');
      await wait(50);
      check('the button runs it', button().getAttribute('aria-label') === 'Run /fork' && !button().disabled, button().getAttribute('aria-label'));
      check('and shows a fork, not the send arrow', button().innerHTML.indexOf('M12 12l0 4') >= 0);
      var hides = calls('peek_hide').length;
      var sends = calls('send_to_session').length;
      enter();
      await wait(50);
      var forks = calls('fork_session');
      check('⏎ forks in the background with the prompt', forks.length === 1 && forks[0][1].key === key && forks[0][1].background === true && forks[0][1].prompt === 'try it with a queue', JSON.stringify(forks.map(function (c) { return c[1]; })));
      check('and sends nothing to the session', calls('send_to_session').length === sends);
      check('clears the box', box().value === '', box().value);
      await until(function () { return noteText().indexOf('Forked in the background') === 0; }, 5000);
      check('says what happened', true);
      check('and leaves the peek open', calls('peek_hide').length === hides);

      failNext = true;
      type(box(), '/fork');
      await wait(50);
      enter();
      await until(function () { return noteText().indexOf('Workspace not trusted') >= 0; }, 5000);
      check('a fork that fails says why', true);
      check('and has what was typed back', box().value === '/fork', box().value);
      check('with no prompt for the copy', calls('fork_session')[1][1].prompt === '', JSON.stringify(calls('fork_session')[1][1]));

      type(box(), '/compact');
      await wait(50);
      check('another /command gets no hint', noteText().indexOf('/fork') < 0, noteText());
      check('and the send arrow', button().innerHTML.indexOf('M12 12l0 4') < 0 && button().getAttribute('aria-label') === 'Send');
      enter();
      await until(function () { return calls('send_to_session').length > sends; }, 5000);
      var sent = calls('send_to_session');
      check('and goes to the session as typed', sent[sent.length - 1][1].text === '/compact', JSON.stringify(sent[sent.length - 1][1]));
      check('not to a fork', calls('fork_session').length === 2);
    } catch (e) {
      out.push('error: ' + e.message);
    }
    say('done: ' + out.join(' | '));
  })();
})();
