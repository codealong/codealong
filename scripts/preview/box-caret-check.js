// Checks where the caret goes when a key pressed with nothing focused sends it to a session's box
// (primitives.tsx BoxText, Peek.tsx): a box that hasn't had the keyboard yet, in a peek up from a
// hover, takes it with the caret at the end of the draft, where the key goes on writing, not at the
// start, where WebKit puts it; and one that had it and lost it gets the caret back where it was
// left. The notch and the waiting window send keys to their box the same way.
//   scripts/preview.sh peek '' scripts/preview/box-caret-check.js
(function () {
  // This runs before the app's script: the peek shows a session with a draft in its box.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && !s.visitor) picked = s; }); });
  if (!picked) { stopped('no session to peek at'); return; }
  var draft = 'A draft written before, on a line of its own';
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.setItem('codealong.draft.' + picked.key, JSON.stringify({ text: draft, at: Date.now() }));

  var box = function () { return document.querySelector('.ca-peek .ca-composer textarea'); };
  var press = function (k) { key(document.body, { key: k }); };

  say('box caret check: waiting for the peek');
  until(function () { return !!box() && box().value === draft; }, 15000, 'the peek’s box with the draft in it')
    .then(function () { return wait(200); })
    .then(function () {
      check('the peek is up from a hover, with nothing focused', document.activeElement !== box(), document.activeElement ? document.activeElement.tagName : 'none');
      press('x');
      return wait(50);
    })
    .then(function () {
      var b = box();
      check('a key pressed with nothing focused gives the box the keyboard', document.activeElement === b);
      check('with the caret at the end of the draft', b.selectionStart === draft.length && b.selectionEnd === draft.length, b.selectionStart + '-' + b.selectionEnd + ' of ' + draft.length);
      b.setSelectionRange(8, 8);
      b.blur();
      return wait(50);
    })
    .then(function () {
      check('the box let go of the keyboard', document.activeElement !== box());
      press('y');
      return wait(50);
    })
    .then(function () {
      var b = box();
      check('the next key gives it back', document.activeElement === b);
      check('with the caret where it was left', b.selectionStart === 8 && b.selectionEnd === 8, b.selectionStart + '-' + b.selectionEnd);
      done();
    })
    .catch(function (e) { stopped(String(e && e.message || e)); });
})();
