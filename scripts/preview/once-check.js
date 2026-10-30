// Checks the panel for a call that can't be allowed just this once (acp.rs `pending`,
// `Pending.noOnce`): a session Codealong runs waits on a call whose agent offers to allow calls
// like it from now on and not once, so the panel shows Deny and Always and no Allow, says why,
// and ⌘⏎ from the box answers nothing.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/once-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'call-1', label: 'Bash · `touch alpha.txt`', input: 'touch alpha.txt', cannot: null, always: true, noOnce: true });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Allowed, and from now on.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = true;
          s.excerpt = 'Bash · `touch alpha.txt`';
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var button = function (text) {
    // A button's text is its label, then any keys it says (Deny⌘⌫).
    var a = approval();
    return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0];
  };
  var answered = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('once check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Deny'); }, 5000);
    })
    .then(function () {
      check('deny and always, no allow', !!button('Deny') && !!button('Always') && !button('Allow'));
      var note = approval().querySelector('.ca-composer-note').textContent;
      check('says it allows only from now on', note.indexOf('only from now on, not just this once') >= 0, note);
      var b = box();
      check('the box is there', !!b);
      if (!b) return;
      b.focus();
      b.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
      return wait(500).then(function () {
        check('⌘⏎ answers nothing', answered().length === 0, JSON.stringify(answered()));
        button('Always').click();
        return until(function () { return answered().length > 0; }, 3000);
      }).then(function () {
        var c = answered()[0][1];
        check('always answers always', c.answer === 'always' && c.id === 'call-1' && c.key === picked.key, JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
