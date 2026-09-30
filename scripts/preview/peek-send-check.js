// Checks sending from the floating bar's peek (SessionReply.tsx, ahead.ts): the box clears and
// the peek closes as you send, without waiting for the send; the peek that shows again has the
// message in its thread as sending, and a second message waits for the first; a send that went as
// a message from Codealong rather than typed puts a note on the bar; and one that fails while no
// box shows says so on the bar and in the next box, with the text back to send.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/peek-send-check.js
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

  // This runs before the app's script: pick a session that doesn't wait on you, with no draft.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  var pickedLane = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !l.hidden) { picked = s; pickedLane = l; } }); });
  if (!picked) { say('done: no session to peek at'); return; }
  var key = picked.key;
  var item = 'codealong.draft.' + key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem(item);

  // Sends wait until the check lets them land, as a slow one on a busy Mac does.
  var real = window.__TAURI_INTERNALS__.invoke;
  var flying = [];
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'send_to_session') return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return new Promise(function (res, rej) { flying.push({ res: res, rej: rej }); });
  };

  var box = function () { return document.querySelector('.ca-peek .ca-composer textarea'); };
  var note = function () { var n = document.querySelector('.ca-peek .ca-composer-note'); return n ? n.textContent : ''; };
  var bubbles = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-peek .ca-thread-said.is-ahead'), function (b) { return b.querySelector('.ca-meta').textContent + ': ' + b.querySelector('.ca-thread-you').textContent; }); };
  var sendButton = function () { return document.querySelector('.ca-peek .ca-composer-send[aria-label="Send"]'); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var enter = function (el) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var calls = function (name) { return window.__calls.filter(function (c) { return c[0] === name; }); };
  var barNotes = function () { return calls('plugin:event|emit_to').filter(function (c) { return c[1] && c[1].event === 'bar-note'; }); };
  var stored = function () { var v = localStorage.getItem(item); return v == null ? null : JSON.parse(v).text; };
  var opens = 1;
  var hide = function () { window.__emit('peek-hidden', null); return until(function () { return !box(); }, 5000); };
  var show = function () {
    opens += 1;
    window.__emit('peek-open', { id: opens, lane: pickedLane, session: picked, terminal: board.apps.terminal, theme: board.theme, room: 608, below: true, focus: false });
    return until(function () { return !!box(); }, 5000);
  };
  var fell = 'Sent as a message from Codealong, not typed into “' + picked.title.trim() + '”: Codealong couldn’t find its Ghostty tab.';

  say('peek send check: waiting for the peek');
  var hides;
  until(function () { return !!box(); }, 20000)
    .then(function () {
      hides = calls('peek_hide').length;
      type(box(), 'Evening then?');
      enter(box());
      return until(function () { return calls('send_to_session').length === 1; }, 5000);
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('the box clears at once and stays open to typing', box() ? box().value === '' && !box().disabled : true, box() ? box().value : 'gone');
      check('the draft goes as it sends', stored() === null, String(stored()));
      check('the peek closes without waiting for the send', calls('peek_hide').length > hides, String(calls('peek_hide').length - hides));
      return hide();
    })
    .then(show)
    .then(function () { return until(function () { return bubbles().length === 1; }, 5000).catch(function () {}); })
    .then(function () {
      check('the peek that shows again has it sending in its thread', bubbles()[0] === 'Sending…: Evening then?', JSON.stringify(bubbles()));
      check('and says so under the box', note() === 'Sending…' && box().value === '' && !box().disabled, note() + ' / ' + box().value);
      type(box(), 'And lunch');
      enter(box());
      return wait(200);
    })
    .then(function () {
      check('a second message waits for the first', calls('send_to_session').length === 1 && bubbles().length === 2, calls('send_to_session').length + ' / ' + JSON.stringify(bubbles()));
      return show();
    })
    .then(function () {
      flying[0].res({ message: fell, opened: false, fellBack: true });
      return until(function () { return calls('send_to_session').length === 2; }, 5000);
    })
    .then(function () { return wait(100); })
    .then(function () {
      var notes = barNotes();
      check('a message that went another way goes on the bar', notes.length === 1 && notes[0][1].payload.message === fell, JSON.stringify(notes.map(function (n) { return n[1].payload; })));
      check('then the second goes', calls('send_to_session')[1][1].text === 'And lunch', JSON.stringify(calls('send_to_session')[1][1]));
      check('the first shows as sent', bubbles()[0] === 'Sent: Evening then?', JSON.stringify(bubbles()));
      // The second fails while no box shows.
      return hide();
    })
    .then(function () {
      flying[1].rej('Ghostty said no.');
      return wait(100);
    })
    .then(function () {
      var notes = barNotes();
      check('one that fails with the peek away says so on the bar', notes.length === 2 && notes[1][1].payload.message === 'Ghostty said no.' && notes[1][1].payload.tone === 'error', JSON.stringify(notes.map(function (n) { return n[1].payload; })));
      check('and is back in the draft', stored() === 'And lunch', String(stored()));
      return show();
    })
    .then(function () {
      check('the next box says how it went', note() === 'Ghostty said no.', note());
      check('with the text back to send', box().value === 'And lunch' && !box().disabled && sendButton() && !sendButton().disabled, box().value);
      check('and it’s gone from the thread', bubbles().every(function (b) { return b.indexOf('And lunch') < 0; }), JSON.stringify(bubbles()));
      say('done: ' + out.join('; '));
    })
    .catch(function (e) { say('done: ' + out.join('; ') + '; stopped: ' + e.message); });
})();
