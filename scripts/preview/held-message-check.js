// Checks the panel for another session's message a Claude Code session holds for you
// (claude/held.rs, `Pending.message`): its row says it holds a message, the panel shows the
// message as prose under a heading that says so, with Deliver and Deny, and Deliver answers
// `allow` for the held message's id. One it can't answer here (several held) still shows the
// message, with only a way to open it where it runs.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/held-message-check.js
// As PREVIEW_INIT, after a line setting `window.__held_screenshot = true`, it leaves the message
// waiting for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  // The first session holds one message; the second holds two.
  var picked = null;
  var several = null;
  var message = 'This message is from another agent, “Review 1021” (Codex):\n\nThe review is done. **Two** findings:\n\n- The migration locks the table\n- One policy reads the wrong column';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var asked = (args || {}).key;
    if (cmd === 'session_pending' && picked && (asked === picked.key || (several && asked === several.key))) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(
        several && asked === several.key
          ? { id: 'held:b2', label: '2 messages, the latest from “Review 1021”', input: message, cannot: 'It holds 2 messages, and only where it runs shows which comes first.', message: true }
          : { id: 'held:a1', label: 'From “Review 1021”', input: message, cannot: null, message: true },
      );
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Delivered the message from “Review 1021”.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (several || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.excerpt = 'Held message from “Review 1021”';
          // As the engine words it (board.rs `session_headline`).
          s.detail = 'from “Review 1021”';
          if (picked) several = { key: s.key, lane: l.id };
          else picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var rowOf = function (p) { return p && document.getElementById('row-' + p.lane + '::' + p.key); };
  var row = function () { return rowOf(picked); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var button = function (text) {
    // A button's text is its label, then any keys it says (Deny⌘⌫).
    var a = approval();
    return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0];
  };
  var answered = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('held message check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      var flag = row().querySelector('.ca-insight');
      check('its row says it holds a message', !!flag && flag.textContent.trim() === 'Message', flag && flag.textContent);
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Deliver'); }, 5000);
    })
    .then(function () {
      var a = approval();
      var head = a.querySelector('.ca-approval-head').textContent;
      check('says it holds a message', head.indexOf('Holds a message for you') >= 0, head);
      var label = a.querySelector('.ca-approval-label');
      check('says whom it is from', !!label && label.textContent.indexOf('From “Review 1021”') >= 0, label && label.textContent);
      var body = a.querySelector('.ca-approval-plan');
      check('the message reads as prose', !!body && body.querySelectorAll('li').length === 2 && !a.querySelector('.ca-approval-input'), body && body.innerHTML);
      check('deliver and deny, nothing else', !!button('Deliver') && !!button('Deny') && !button('Allow') && !button('Always'));
      var note = a.querySelector('.ca-composer-note').textContent;
      check('says what deny does', note.indexOf('its sender is told it was declined') >= 0, note);
      if (window.__held_screenshot) return 'stay';
      button('Deliver').click();
      return until(function () { return answered().length > 0; }, 3000).then(function () {
        var c = answered()[0][1];
        check('deliver answers allow for the held message', c.answer === 'allow' && c.id === 'held:a1' && c.key === picked.key, JSON.stringify(c));
      });
    })
    .then(function (stay) {
      if (stay) return;
      // Several held: the message still shows, to read before going where it runs.
      if (!rowOf(several)) {
        note('no second Claude Code session to hold several messages');
        return;
      }
      rowOf(several).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      rowOf(several).click();
      return until(function () { var a = approval(); return a && a.classList.contains('is-elsewhere'); }, 5000)
        .then(function () {
          var a = approval();
          check('several held still show the message', !!a.querySelector('.ca-approval-plan'));
          check('and only a way to open it', !button('Deliver') && !button('Deny') && !!button('Open in'), a.textContent);
          var note = a.querySelector('.ca-composer-note').textContent;
          check('saying why', note.indexOf('only where it runs shows which comes first') >= 0, note);
        });
    })
    .then(done)
    .catch(stopped);
})();
