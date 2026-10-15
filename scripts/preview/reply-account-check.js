// Checks that a session's account is said in its reply box (SessionReply.tsx, primitives.tsx
// `AccountLabel`), not in its header: at the left of the box's foot, just before how full its
// context is and at its size and color, as typed, with its harness on hover, the model and Send
// at the right; and that in a box short of room the account and the meter stay whole and on the
// line, and a name too long for it is cut rather than push the meter or the buttons out. Every
// session here says its context is 30% full, so the meter is there to stand beside, in its quiet
// color.
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
          s.context = { used: 300000, window: 1000000 };
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
  var label = function () { return q('.ca-composer-who .ca-account'); };

  until(function () {
    // In Sessions, a session is shown once a row is picked.
    if (!label() && !q('.ca-sessinsp-title')) {
      var row = q('.ca-row[role="option"]');
      if (row) row.click();
    }
    return label();
  }, 20000, 'the account in the box')
    .then(function (label) {
      var who = label.closest('.ca-composer-who');
      var field = who.closest('.ca-composer-field');
      var buttons = q('.ca-composer-buttons', field);
      var title = q('.ca-sessinsp-title').getAttribute('title');
      var s = sessions.filter(function (x) { return x.title === title; })[0];
      if (!s) return stopped('no session on the board is titled ' + title);
      var meter = who.querySelector('.ca-context');
      check('it is just before the context meter, which ends the row', !!meter && label.nextElementSibling === meter && who.lastElementChild === meter, who.innerHTML.slice(0, 160));
      var room = field.getBoundingClientRect();
      var at = label.getBoundingClientRect();
      check('it sits at the left of the box’s foot', at.left < room.left + room.width / 2 && at.right <= buttons.getBoundingClientRect().left, Math.round(at.left - room.left) + ' of ' + Math.round(room.width));
      check('under where you write', at.top >= q('textarea', field).getBoundingClientRect().bottom - 1, Math.round(at.top));
      check('it names the account as typed', label.textContent === s.account, label.textContent + ' for ' + s.account);
      check('at the size of the meter beside it', getComputedStyle(label).fontSize === getComputedStyle(meter).fontSize, getComputedStyle(label).fontSize + ' beside ' + getComputedStyle(meter).fontSize);
      check('and in its color', getComputedStyle(label).color === getComputedStyle(meter).color, getComputedStyle(label).color + ' beside ' + getComputedStyle(meter).color);
      check('its harness is on hover', /\(.+\)$/.test(label.getAttribute('title') || ''), label.getAttribute('title'));
      check('the header doesn’t name it', !q('.ca-sessinsp-meta .ca-account'), q('.ca-sessinsp-meta').textContent);

      // Short of room: a box as narrow as a card gets.
      q('.ca-composer').style.width = '300px';
      var name = label.querySelector('.ca-account-name');
      var fits = function () {
        var r = field.getBoundingClientRect();
        var m = meter.getBoundingClientRect();
        var b = buttons.getBoundingClientRect();
        return { r: r, m: m, b: b, ok: m.right <= b.left + 1 && b.right <= r.right + 1 && m.height < 24 };
      };
      var f = fits();
      check('squeezed, the meter stays on its line, short of the buttons', f.ok, Math.round(f.m.right) + ' to ' + Math.round(f.b.left) + ', ' + Math.round(f.m.height) + 'px tall');
      check('squeezed, the buttons stay in the box', f.b.right <= f.r.right + 1, Math.round(f.b.right) + ' past ' + Math.round(f.r.right));

      // A name longer than the row has room for is cut, and the meter and the buttons stay in.
      name.textContent = 'an-account-named-at-very-great-length-for-this-box';
      f = fits();
      check('a name too long for the row is cut', !whole(name), name.scrollWidth + ' in ' + name.clientWidth);
      check('and the meter and the buttons stay in the box', f.ok, Math.round(f.m.right) + ' to ' + Math.round(f.b.left) + ', ' + Math.round(f.b.right) + ' in ' + Math.round(f.r.right));
    })
    .then(done)
    .catch(stopped);
})();
