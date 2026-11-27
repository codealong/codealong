// Checks a session that moved to another account (`Session.movedTo`, origins.rs
// `Store::mark_moves`) as its panel shows it once the engine says so, whichever window moved it
// and however long ago: the box under its conversation is its copy's (SessionPanel.tsx,
// MovedNote.tsx), the line above it says where it went, its chip shows the copy, and what's
// written there is sent to the copy, not to the session the engine won't send to.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/moved-box-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ message: 'Sent.', opened: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var now = Date.now();
      var listed = function (l, s) { return !l.hidden && !l.device && !s.visitor && !s.helper && !s.headless; };
      // A session and another of its worktree's on another account: the first moved, archived
      // and stopped as a move leaves it, and the second its copy, running at its prompt.
      board.lanes.some(function (l) {
        var from = l.sessions.find(function (s) { return listed(l, s); });
        var to = from && l.sessions.find(function (s) { return listed(l, s) && s.account !== from.account; });
        if (!to) return false;
        Object.assign(from, { movedTo: to.key, archived: true, live: false, status: 'stopped' });
        Object.assign(to, { archived: false, live: true, status: 'idle', asks: false, lastActivity: now - 60e3, statusSince: now - 60e3 });
        picked = { lane: l.id, from: { key: from.key, title: from.title, account: from.account }, to: { key: to.key, title: to.title, account: to.account } };
        return true;
      });
      if (picked) board.pinned = (board.pinned || []).filter(function (k) { return k !== picked.from.key && k !== picked.to.key; });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var note = function () { return document.querySelector('.ca-sessinsp .ca-composer .ca-movednote'); };
  var sends = function () { return window.__calls.filter(function (c) { return c[0] === 'send_to_session'; }); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };

  say('moved box check: waiting for the board');
  until(function () { return !!picked && !!document.querySelector('[role="listbox"]'); }, 20000, 'a worktree with sessions on two accounts')
    .then(function () { return wait(300); })
    .then(function () {
      window.__emit('reveal', { kind: 'session', laneId: picked.lane, key: picked.from.key });
      return until(function () { return title() === picked.from.title && !!box(); }, 5000, 'the moved session shown');
    })
    .then(function () {
      var n = note();
      check('the line above its box says where it went', !!n && n.textContent.indexOf('Moved to ' + picked.to.account) === 0 && /goes to it there/.test(n.textContent), n ? n.textContent : 'none');
      var said = box().getAttribute('aria-placeholder') || '';
      var after = said.charAt(('Send to ' + picked.to.account).length);
      check('the box is the copy’s: it says it sends to the copy’s account', said.indexOf('Send to ' + picked.to.account) === 0 && !/[\w-]/.test(after), said);
      var account = document.querySelector('.ca-sessinsp .ca-composer-buttons .ca-account-name');
      check('and its account chip is the copy’s', !!account && account.textContent === picked.to.account, account ? account.textContent : 'none');
      type(box(), 'Carry on with the next step');
      box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      return until(function () { return sends().length === 1; }, 5000, 'the send');
    })
    .then(function () {
      var sent = sends()[0][1];
      check('what’s written there goes to the copy', sent.key === picked.to.key && sent.text === 'Carry on with the next step', JSON.stringify(sent));
      var chip = note() && note().querySelector('.ca-movedchip');
      check('its chip shows the copy, as the row’s does', !!chip && chip.tagName === 'BUTTON', chip ? chip.outerHTML.slice(0, 120) : 'none');
      chip.click();
      return until(function () { return title() === picked.to.title; }, 3000, 'the copy shown');
    })
    .then(function () {
      check('a click on the chip shows the copy', title() === picked.to.title, title());
      check('whose own box has no such line', !note());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
