// Checks that a send still on its way survives the floating bar's peek closing and opening again
// (SessionReply.tsx): the box that shows again says it's sending and can't send the same text a
// second time, then hears how it went; a send that went as a message from Codealong rather than
// typed puts a note on the bar; and one that fails while no box shows says so in the next box,
// with the text still there to send.
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
  var sendButton = function () { return Array.prototype.find.call(document.querySelectorAll('.ca-peek .ca-composer button'), function (b) { return /Send/.test(b.textContent); }); };
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
  until(function () { return !!box(); }, 20000)
    .then(function () {
      type(box(), 'Evening then?');
      enter(box());
      return until(function () { return calls('send_to_session').length === 1; }, 5000);
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('sending shows', note() === 'Sending…' && box().disabled, note());
      return hide();
    })
    .then(show)
    .then(function () {
      check('the box that shows again is still sending', note() === 'Sending…' && box().disabled && box().value === 'Evening then?', note() + ' / ' + box().value);
      enter(box());
      if (sendButton()) sendButton().click();
      return wait(200);
    })
    .then(function () {
      check('it doesn’t send the same text twice', calls('send_to_session').length === 1, String(calls('send_to_session').length));
      var hides = calls('peek_hide').length;
      flying[0].res({ message: fell, opened: false, fellBack: true });
      return until(function () { return calls('peek_hide').length > hides; }, 5000);
    })
    .then(function () {
      var notes = barNotes();
      check('a message that went another way goes on the bar', notes.length === 1 && notes[0][1].payload.message === fell, JSON.stringify(notes.map(function (n) { return n[1].payload; })));
      check('the peek closes once it went', true);
      check('sending cleared the draft', stored() === null && box().value === '', String(stored()));
      // A send that fails while no box shows.
      return hide();
    })
    .then(show)
    .then(function () {
      type(box(), 'Second one');
      enter(box());
      return until(function () { return calls('send_to_session').length === 2; }, 5000);
    })
    .then(hide)
    .then(function () {
      flying[1].rej('Ghostty said no.');
      return wait(100);
    })
    .then(show)
    .then(function () {
      check('the next box says how it went', note() === 'Ghostty said no.', note());
      check('with the text still there to send', box().value === 'Second one' && !box().disabled && sendButton() && !sendButton().disabled, box().value);
      check('and nothing more on the bar', barNotes().length === 1, String(barNotes().length));
      say('done: ' + out.join('; '));
    })
    .catch(function (e) { say('done: ' + out.join('; ') + '; stopped: ' + e.message); });
})();
