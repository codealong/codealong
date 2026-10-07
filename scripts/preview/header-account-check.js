// Checks that a session's header names its account (SessionHeader.tsx, primitives.tsx
// `AccountLabel`): last on the line under the title, as typed, with its harness on hover; and that
// in a header short of room the worktree and branch give way, while the account stays whole.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/header-account-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/header-account-check.js
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var sessions = [];
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { sessions.push(s); }); });
  var whole = function (el) { return el.scrollWidth <= el.clientWidth + 1; };

  until(function () { return q('.ca-sessinsp-meta .ca-account'); }, 20000, "the header's account")
    .then(function (label) {
      var meta = q('.ca-sessinsp-meta');
      var title = q('.ca-sessinsp-title').getAttribute('title');
      var s = sessions.filter(function (x) { return x.title === title; })[0];
      if (!s) return stopped('no session on the board is titled ' + title);
      check('it is last on the line under the title', meta.lastElementChild === label, meta.textContent);
      check('it names the account as typed', label.textContent === s.account, label.textContent + ' for ' + s.account);
      check('its harness is on hover', /\(.+\)$/.test(label.getAttribute('title') || ''), label.getAttribute('title'));

      // Short of room: a header as narrow as its line can get.
      q('.ca-sessinsp-head').style.width = '300px';
      var name = label.querySelector('.ca-account-name');
      var lane = q('.ca-sessinsp-meta .ca-sessinsp-lane span');
      var room = meta.getBoundingClientRect();
      var at = label.getBoundingClientRect();
      check('squeezed, it stays inside the line', at.right <= room.right + 1 && at.left >= room.left, Math.round(at.right) + ' past ' + Math.round(room.right));
      check('squeezed, it stays whole', whole(name), name.scrollWidth + ' in ' + name.clientWidth);
      check('squeezed, the worktree gives way instead', !whole(lane), lane.textContent);
    })
    .then(done)
    .catch(stopped);
})();
