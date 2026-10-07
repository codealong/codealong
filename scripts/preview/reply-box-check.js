// Checks the shape of a session's reply box (SessionReply.tsx, primitives.tsx `BoxText`), in the
// floating bar's peek: the text runs across the box's whole width, with the model and Send in a
// row under it; the box grows with what's written, wrapped lines as well as new ones, up to eight
// lines, with no scrollbar while it fits; and past that it scrolls, its scrollbar at the box's
// right edge, in a lane kept for it, so the text doesn't move as the scrollbar comes.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/reply-box-check.js
(function () {
  // This runs before the app's script: pick a session that doesn't wait on you, with no draft.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && s.status !== 'needs' && !s.archived && !l.hidden) picked = s; }); });
  if (!picked) { stopped('no session to peek at'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var LINE = 19;
  var box = function () { return q('.ca-peek .ca-composer-text > textarea'); };
  var field = function () { return q('.ca-peek .ca-composer-field'); };
  var buttons = function () { return q('.ca-peek .ca-composer-buttons'); };
  var lines = function () { return Math.round((box().clientHeight - 12) / LINE); };
  var fits = function () { return box().scrollHeight <= box().clientHeight + 1; };
  var words = 'The text reaches the right edge of the box, and the buttons sit in a row under it. ';
  var settle = function () { return wait(100); };
  // How wide the text is before the box scrolls.
  var width = 0;

  say('reply box check: waiting for the peek');
  until(function () { return !!box() && !!buttons(); }, 20000)
    .then(function () {
      var f = field().getBoundingClientRect();
      var t = box().getBoundingClientRect();
      check('the empty box is one line, its hint over it', lines() === 1 && !!q('.ca-peek .ca-composer-hint'), lines() + ' lines');
      check('its text runs across the whole box', Math.abs(t.left - f.left - 1) < 1 && Math.abs(f.right - 1 - t.right) < 1, Math.round(t.left - f.left) + ' / ' + Math.round(f.right - t.right));
      var b = buttons().getBoundingClientRect();
      check('Send sits under the text, at the right', b.top >= t.bottom - 0.5 && Math.abs(f.right - 1 - b.right - 5) < 1.5, Math.round(b.top) + ' under ' + Math.round(t.bottom) + ', ' + Math.round(f.right - b.right) + ' from the right');
      width = box().clientWidth;
      type(box(), words + words + words + words);
      return settle();
    })
    .then(function () {
      check('a paragraph grows the box with its wrapped lines, with nothing to scroll', lines() > 2 && lines() < 8 && fits() && !q('.ca-peek .ca-composer-hint'), lines() + ' lines, ' + box().scrollHeight + ' of ' + box().clientHeight);
      // Word by word up to seven lines: the copy that sizes the box wraps where the text does,
      // with a scrollbar or without, so no word on the way scrolls it.
      var every = (words + words + words + words + words + words + words).split(' ');
      var short = [];
      var step = function (n) {
        if (n > every.length || lines() >= 7) return null;
        type(box(), every.slice(0, n).join(' '));
        return wait(5).then(function () {
          if (!fits()) short.push(n + ' words: ' + box().scrollHeight + ' of ' + box().clientHeight);
          return step(n + 1);
        });
      };
      return Promise.resolve(step(1)).then(function () {
        check('no word on the way to seven lines scrolls it', short.length === 0 && lines() >= 6, short.slice(0, 3).join('; ') || lines() + ' lines');
        type(box(), 'One\nTwo\n');
        return settle();
      });
    })
    .then(function () {
      check('a last line that’s empty has its room', lines() === 3 && fits(), lines() + ' lines');
      var many = '';
      for (var i = 0; i < 6; i++) many += words + words + '\n';
      type(box(), many);
      return settle();
    })
    .then(function () {
      var f = field().getBoundingClientRect();
      var t = box().getBoundingClientRect();
      check('past eight lines the box stops growing and scrolls', lines() === 8 && !fits(), lines() + ' lines, ' + box().scrollHeight + ' of ' + box().clientHeight);
      check('its scrollbar is at the box’s edge', Math.abs(f.right - 1 - t.right) < 1, Math.round(f.right - t.right) + ' from the edge');
      check('and the text keeps its width as it starts to scroll', box().clientWidth === width, width + ' → ' + box().clientWidth);
      check('Send stays under it', buttons().getBoundingClientRect().top >= t.bottom - 0.5, Math.round(buttons().getBoundingClientRect().top) + ' / ' + Math.round(t.bottom));
      type(box(), '');
      return settle();
    })
    .then(function () {
      check('emptied, it’s one line again', lines() === 1, lines() + ' lines');
    })
    .then(done)
    .catch(stopped);
})();
