// Checks that a session's account is said in its reply box (SessionReply.tsx, SessionAgent.tsx),
// not in its header: first in the chip at the right of the box's foot, after how full its context
// is and before Send, as typed, with its harness on hover; where it runs is at the left, after its
// mode. In a box short of room, or beside buttons wider than the chip's own, the left and the right
// go on lines of their own rather than run over each other, and stay in the box; a name too long
// for the chip is cut rather than push Send out. Every session here says its context is 30% full,
// so the meter is there to stand beside.
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
  var label = function () { return q('.ca-composer-buttons .ca-account-name'); };

  until(function () {
    // In Sessions, a session is shown once a row is picked.
    if (!label() && !q('.ca-sessinsp-title')) {
      var row = q('.ca-row[role="option"]');
      if (row) row.click();
    }
    return label();
  }, 20000, 'the account in the box')
    .then(function (label) {
      var buttons = label.closest('.ca-composer-buttons');
      var field = buttons.closest('.ca-composer-field');
      var who = q('.ca-composer-who', field);
      var chip = label.closest('.ca-modelmenu-chip, .ca-modelmenu-said');
      var title = q('.ca-sessinsp-title').getAttribute('title');
      var s = sessions.filter(function (x) { return x.title === title; })[0];
      if (!s) return stopped('no session on the board is titled ' + title);
      var meter = q('.ca-context', buttons);
      var holder = chip.closest('.ca-modelmenu') || chip;
      check('it starts the chip just after the context meter, beside Send', !!meter && meter.nextElementSibling === holder && chip.textContent.indexOf(s.account) === 0 && !!q('.ca-composer-send', buttons), buttons.innerHTML.slice(0, 160));
      var room = field.getBoundingClientRect();
      var at = label.getBoundingClientRect();
      check('it sits at the right of the box’s foot', at.left > room.left + room.width / 3, Math.round(at.left - room.left) + ' of ' + Math.round(room.width));
      check('under where you write', at.top >= q('textarea', field).getBoundingClientRect().bottom - 1, Math.round(at.top));
      check('it names the account as typed', label.textContent === s.account, label.textContent + ' for ' + s.account);
      check('its harness is on hover', /\(.+\)/.test(chip.getAttribute('title') || ''), chip.getAttribute('title'));
      check('the header doesn’t name it', !q('.ca-sessinsp-meta .ca-account'), q('.ca-sessinsp-meta').textContent);
      var place = q('.ca-boxplace', who);
      check('where it runs is at the left', !!place && place.textContent.trim().length > 0 && place.getBoundingClientRect().right <= at.left, place ? place.textContent : 'none');

      // Short of room: a box as narrow as a card gets, beside buttons wider than the chip's own.
      q('.ca-composer').style.width = '360px';
      var wide = document.createElement('span');
      wide.style.cssText = 'display:inline-block;width:190px;height:22px';
      buttons.insertBefore(wide, buttons.firstChild);
      // The left and the buttons side by side, or the buttons on a line under it; and both in the box.
      var fits = function () {
        var r = field.getBoundingClientRect();
        var w = who.getBoundingClientRect();
        var b = buttons.getBoundingClientRect();
        var apart = w.right <= b.left + 1 || w.bottom <= b.top + 1;
        return { r: r, w: w, b: b, ok: apart && b.right <= r.right + 1 && w.right <= r.right + 1 };
      };
      var where = function (f) { return 'left ' + Math.round(f.w.left) + '–' + Math.round(f.w.right) + ' at ' + Math.round(f.w.top) + ', buttons ' + Math.round(f.b.left) + '–' + Math.round(f.b.right) + ' at ' + Math.round(f.b.top) + ', box to ' + Math.round(f.r.right); };
      var f = fits();
      check('squeezed, the buttons don’t run over the left, and both stay in the box', f.ok, where(f));
      check('squeezed, the account stays whole', whole(label) && label.clientWidth > 0, label.scrollWidth + ' in ' + label.clientWidth);
      check('and the buttons are still at the right', Math.abs(f.b.right - (f.r.right - 5)) <= 2, where(f));
      wide.remove();

      // A name longer than the chip has room for is cut, and Send stays in the box.
      label.textContent = 'an-account-named-at-very-great-length-for-this-box';
      f = fits();
      var send = q('.ca-composer-send', buttons).getBoundingClientRect();
      check('a name too long for the chip is cut', !whole(label) || !whole(label.parentElement), label.scrollWidth + ' in ' + label.clientWidth);
      check('and Send stays in the box', send.right <= f.r.right + 1, Math.round(send.right) + ' in ' + Math.round(f.r.right));
    })
    .then(done)
    .catch(stopped);
})();
