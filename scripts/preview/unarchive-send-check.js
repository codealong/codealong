// Checks that a message sent to an archived session takes it out of the archive as it goes
// (ahead.ts `sessionAhead`, actions.rs `unarchive_to_send`): shown from Sessions' Archived fold,
// a message from its box moves its row out of the fold into the list, still selected, with its
// conversation still in the panel.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/unarchive-send-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  var TITLE = 'Sent from the archive';
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var now = Date.now();
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked ? s.key !== picked.key : s.visitor || s.archived || s.helper || l.hidden || l.device) return;
          if (!picked) picked = { key: s.key, lane: l.id };
          // Running at its prompt, so a message goes to it rather than resuming it.
          Object.assign(s, { title: TITLE, archived: true, live: true, status: 'idle', lastActivity: now - 60e3, statusSince: now - 60e3 });
        });
      });
      board.pinned = (board.pinned || []).filter(function (k) { return !picked || k !== picked.key; });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function () { return document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var sends = function () { return window.__calls.filter(function (c) { return c[0] === 'send_to_session'; }); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var where = function () { var r = row(); return r ? (r.closest('.ca-archived') ? 'in the Archived fold' : 'in the list') + (r.getAttribute('aria-selected') === 'true' ? ', selected' : '') : 'no row'; };

  say('unarchive send check: waiting for the board');
  until(function () { return !!picked && !!document.querySelector('[role="listbox"]'); }, 20000)
    .catch(function () { throw new Error('no session on the board to archive'); })
    .then(function () { return wait(300); })
    .then(function () {
      window.__emit('reveal', { kind: 'session', laneId: picked.lane, key: picked.key });
      return until(function () { return title() === TITLE && !!box(); }, 5000);
    })
    .then(function () {
      check('the archived session shows from the Archived fold', where() === 'in the Archived fold, selected', where());
      type(box(), 'Picking this up again');
      box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      return until(function () { return sends().length === 1; }, 5000);
    })
    .then(function () {
      var sent = sends()[0][1];
      check('the message goes to it', sent.key === picked.key && sent.text === 'Picking this up again', JSON.stringify(sent));
      return until(function () { return where() === 'in the list, selected'; }, 3000).catch(function () {});
    })
    .then(function () {
      check('its row leaves the Archived fold for the list, still selected', where() === 'in the list, selected', where());
      check('with its conversation still in the panel', title() === TITLE, title());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + where()); })
    .then(done);
})();
