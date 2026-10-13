// Checks that a session's account is said under its reply box (SessionReply.tsx, primitives.tsx
// `AccountLabel`), not in its header: at the right of the line under the box, just before how
// full its context is, as typed, with its harness on hover; and that in a box short of room what
// the box says gives way, while the account and the meter stay whole. Every session here says
// its context, so the meter is there to stand beside.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/reply-account-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/reply-account-check.js
//   scripts/preview.sh sessions '' scripts/preview/reply-account-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var answer = real(cmd, args, options);
    if (cmd !== 'get_board') return answer;
    return answer.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          s.context = { used: 820093, window: 1000000 };
        });
      });
      return board;
    });
  };
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var sessions = [];
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { sessions.push(s); }); });
  var whole = function (el) { return el.scrollWidth <= el.clientWidth + 1; };
  var label = function () { return q('.ca-composer-under .ca-account'); };

  until(function () {
    // In Sessions, a session is shown once a row is picked.
    if (!label() && !q('.ca-sessinsp-title')) {
      var row = q('.ca-row[role="option"]');
      if (row) row.click();
    }
    return label();
  }, 20000, 'the account under the box')
    .then(function (label) {
      var under = label.closest('.ca-composer-under');
      var title = q('.ca-sessinsp-title').getAttribute('title');
      var s = sessions.filter(function (x) { return x.title === title; })[0];
      if (!s) return stopped('no session on the board is titled ' + title);
      var meter = under.querySelector('.ca-context');
      check('it is just before the context meter, at the end of the line', !!meter && label.nextElementSibling === meter && under.lastElementChild === meter, under.innerHTML.slice(0, 160));
      var room = under.getBoundingClientRect();
      var at = label.getBoundingClientRect();
      check('it sits at the right of the line', at.left > room.left + room.width / 2, Math.round(at.left - room.left) + ' of ' + Math.round(room.width));
      check('it names the account as typed', label.textContent === s.account, label.textContent + ' for ' + s.account);
      check('its harness is on hover', /\(.+\)$/.test(label.getAttribute('title') || ''), label.getAttribute('title'));
      check('the header doesn’t name it', !q('.ca-sessinsp-meta .ca-account'), q('.ca-sessinsp-meta').textContent);

      // Short of room: a box as narrow as a card gets, with something long to say.
      q('.ca-composer').style.width = '260px';
      var note = under.querySelector('.ca-composer-note');
      if (!note) {
        note = document.createElement('span');
        note.className = 'ca-composer-note';
        under.insertBefore(note, under.firstChild);
      }
      note.textContent = '⌘⏎ send + next · ⌃⏎ send now, into the turn it’s taking';
      var name = label.querySelector('.ca-account-name');
      room = under.getBoundingClientRect();
      at = label.getBoundingClientRect();
      var m = meter.getBoundingClientRect();
      check('squeezed, it stays inside the line', at.left >= room.left && m.right <= room.right + 1, Math.round(m.right) + ' past ' + Math.round(room.right));
      check('squeezed, it stays whole', whole(name), name.scrollWidth + ' in ' + name.clientWidth);
      check('squeezed, the meter stays on one line', m.height < 24, Math.round(m.height) + 'px tall');
      check('squeezed, what the box says gives way instead', note.getBoundingClientRect().width < room.width - at.width - m.width, Math.round(note.getBoundingClientRect().width) + 'px wide');
    })
    .then(done)
    .catch(stopped);
})();
