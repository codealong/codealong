// Checks /fork in a side conversation's box (SessionReply.tsx, slash.ts `commandWords`, side.ts
// `shownSide`), in the bar's peek: with a side conversation open, a `/` lists /fork alone, with
// what it does there, and no skills; ⏎ on `/fork <what to do>` asks for a background fork with it
// and the open side conversation's id, asks the copy nothing, clears the box and leaves the side
// conversation showing; one from before being read is the one handed over; and with only the list
// of them showing, none is.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/side-fork-check.js
(function () {
  // A Claude Code session to peek at, idle and taking messages.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to peek at'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  localStorage.removeItem('codealong.draft.side:' + key);
  localStorage.removeItem('codealong.sideview.' + key);
  window.__past = [{ id: 'old', key: key, title: 'x', startedAt: Date.now() - 7200000, lastAt: Date.now() - 7200000, turns: [{ question: 'is the queue tested?', answer: 'In `send.rs`.', done: true }] }];
  var calm = { status: 'idle', live: true, stoppable: false, steerable: false, stalledSince: null, asks: false };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'fork_session') {
      window.__calls.push([cmd, arguments[1]]);
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
  var strip = function () { return document.querySelector('.ca-sidestrip'); };
  var turns = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-sidestrip-turn'), function (t) { return t.textContent; }); };
  var listed = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-slash-row'), function (r) { return r.textContent; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var headButton = function (label) { return Array.prototype.filter.call(strip().querySelectorAll('.ca-sidestrip-head button'), function (b) { return b.getAttribute('aria-label') === label; })[0]; };
  var forks = function () { return calls('fork_session').map(function (c) { return c[1]; }); };

  (async function () {
    try {
      await until(box, 20000);
      type(box(), '/side what’s left before the release?');
      await wait(50);
      enter();
      await until(function () { return turns().length === 1; }, 5000);
      var open = window.__sides[key];
      window.__sides[key] = Object.assign({}, open, { running: false, turns: [{ question: 'what’s left before the release?', answer: 'Three PRs to merge.', done: true }] });
      window.__emit('side', window.__sides[key]);
      await until(function () { return turns()[0].indexOf('Three PRs to merge.') > 0; }, 5000);

      type(box(), '/');
      await wait(50);
      check('a / in the side box lists /fork alone, with what it does there', listed().length === 1 && /^\/fork what to doCodealong: a copy of the session does it in the background, told what was said here/.test(listed()[0]), listed().join(' | '));
      type(box(), '/fork merge them, then tell the main session');
      await wait(50);
      check('what the box says names it', noteText().indexOf('/fork what to do: a copy of the session does it in the background, told what was said here') === 0, noteText());
      check('and the button runs it', button().getAttribute('aria-label') === 'Run /fork' && !button().disabled, button().getAttribute('aria-label'));
      var asks = calls('side_ask').length;
      var sends = calls('send_to_session').length;
      enter();
      await until(function () { return forks().length === 1; }, 5000);
      check('⏎ forks in the background with what to do and the open side conversation', forks()[0].key === key && forks()[0].background === true && forks()[0].prompt === 'merge them, then tell the main session' && forks()[0].side === open.id, JSON.stringify(forks()[0]));
      check('and asks the copy nothing, nor the session', calls('side_ask').length === asks && calls('send_to_session').length === sends);
      check('clears the box', box().value === '', box().value);
      await until(function () { return noteText().indexOf('Forked in the background') === 0; }, 5000);
      check('says what happened, and the side conversation still shows', turns().length === 1 && box().getAttribute('aria-placeholder') === 'Ask on the side', turns().join(' / '));

      headButton('Earlier').click();
      await until(function () { return strip().querySelectorAll('.ca-sidestrip-kept').length > 0; }, 5000);
      type(box(), '/fork do it anyway');
      await wait(50);
      enter();
      await until(function () { return forks().length === 2; }, 5000);
      check('with only the list showing, none is handed over', forks()[1].side === undefined && forks()[1].prompt === 'do it anyway', JSON.stringify(forks()[1]));
      var old = Array.prototype.filter.call(strip().querySelectorAll('.ca-sidestrip-kept'), function (r) { return r.textContent.indexOf('is the queue tested?') === 0; })[0];
      old.click();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('is the queue tested?') === 0; }, 5000);
      type(box(), '/fork add the missing test');
      await wait(50);
      enter();
      await until(function () { return forks().length === 3; }, 5000);
      check('one from before being read is the one handed over', forks()[2].side === 'old' && forks()[2].prompt === 'add the missing test', JSON.stringify(forks()[2]));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
