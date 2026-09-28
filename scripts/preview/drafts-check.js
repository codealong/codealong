// Checks that what you write to a session stays with it (drafts.ts, SessionReply.tsx), in the
// floating bar's peek: the draft kept for a session fills its box, typing keeps it, the box stays
// the same box with the same text as the session starts and stops waiting on you (Send waits
// meanwhile, and ⏎ sends nothing), another window's change to the draft shows, and sending
// clears it.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/drafts-check.js
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

  // This runs before the app's script: pick a session that doesn't wait on you, and give it a
  // draft, as an earlier peek would have left it.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !l.hidden) picked = s; }); });
  if (!picked) { say('done: no session to peek at'); return; }
  var key = picked.key;
  var item = 'codealong.draft.' + key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.setItem(item, JSON.stringify({ text: 'Left from before,', at: Date.now() }));

  var stored = function () { var v = localStorage.getItem(item); return v == null ? null : JSON.parse(v).text; };
  var box = function () { return document.querySelector('.ca-peek .ca-composer textarea'); };
  var send = function () { return Array.prototype.find.call(document.querySelectorAll('.ca-peek .ca-composer button'), function (b) { return /Send/.test(b.textContent); }); };
  var waitingShown = function () { return !!document.querySelector('.ca-peek .ca-composer.is-waiting, .ca-peek .ca-composer.is-approval'); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var enter = function (el) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var sends = function () { return window.__calls.filter(function (c) { return c[0] === 'send_to_session'; }); };
  var push = function (status) {
    var b = JSON.parse(xhr.responseText);
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) { s.status = status; s.asks = false; s.statusSince = Date.now(); } }); });
    window.__emit('board', b);
  };

  say('drafts check: waiting for the peek');
  until(function () { return !!box(); }, 20000)
    .then(function () {
      var first = box();
      check('the kept draft fills the box', first.value === 'Left from before,', first.value);
      type(first, 'Left from before, and more');
      check('typing keeps it', stored() === 'Left from before, and more', stored());
      push('needs');
      return until(waitingShown, 5000).then(function () { return first; });
    })
    .then(function (first) {
      check('the box stays while it waits on you', box() === first && first.value === 'Left from before, and more', box() === first ? first.value : 'another box');
      check('Send waits', send() && send().disabled);
      enter(first);
      return wait(200).then(function () { return first; });
    })
    .then(function (first) {
      check('⏎ sends nothing while it waits', sends().length === 0);
      window.dispatchEvent(new StorageEvent('storage', { key: item, newValue: JSON.stringify({ text: 'Written in the quick panel', at: Date.now() }) }));
      return wait(100).then(function () { return first; });
    })
    .then(function (first) {
      check('another window’s change shows', first.value === 'Written in the quick panel', first.value);
      push('working');
      return until(function () { return !waitingShown(); }, 5000).then(function () { return first; });
    })
    .then(function (first) {
      check('the same box once it stops waiting', box() === first && first.value === 'Written in the quick panel');
      check('Send is back', send() && !send().disabled);
      enter(first);
      return until(function () { return sends().length > 0; }, 5000);
    })
    .then(function () {
      var s = sends()[0][1];
      check('⏎ sends it', s.key === key && s.text === 'Written in the quick panel', JSON.stringify(s));
      return wait(200);
    })
    .then(function () {
      check('sending clears the draft', stored() === null, String(stored()));
      say('done: ' + out.join('; '));
    })
    .catch(function (e) { say('done: ' + out.join('; ') + '; stopped: ' + e.message); });
})();
