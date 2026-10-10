// Checks suggested replies (SuggestedReplies.tsx, SessionReply.tsx, `codealong session suggest`):
// a session with replies another session suggested has a Suggested chip on its row, whose tooltip
// lists them, and its panel shows them above the box, with who suggested them and the note. A
// click sends one as it is, leaving the draft in the box, and they go at once; ⌥-click puts one in
// the box after the draft and sends nothing; ↑ in the empty box goes to the nearest one and ⏎
// there sends it; a reply's menu has Send, Change before sending and Dismiss suggestions; and
// Dismiss asks the engine to take them away, and they go at once.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/suggest-check.js
(function () {
  // This runs before the app's script: the board it gets has one turn that ended, made from a
  // real session on this device, with three suggested replies, and nothing else waiting.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var replies = ['Yes, push it and open the PR', 'Run the whole test suite first, then push', 'No: rebase onto main before anything else'];
  var suggestions = { replies: replies, note: 'It asks whether to push; the tests it ran were only the engine’s.', by: 'claude:claude:supervisor', byTitle: 'Supervisor', at: now - 60e3 };
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.suggestions = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || s.hosted || (picked && picked.key !== s.key)) return;
          picked = picked || { key: s.key, lane: l.id };
          localStorage.removeItem('codealong.draft.' + s.key);
          s.status = 'unread';
          s.statusSince = now - 5 * 60e3;
          s.lastActivity = now - 5 * 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.title = 'Suggest A';
          s.suggestions = suggestions;
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function () { return document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var box = function () { return q('.ca-sessinsp .ca-composer textarea'); };
  var strip = function () { return q('.ca-sessinsp .ca-suggest'); };
  var rows = function () { return all('.ca-sessinsp .ca-suggest-reply'); };
  var note = function () { var n = q('.ca-sessinsp .ca-composer-note'); return n ? n.textContent : ''; };
  var sends = function () { return sent('send_to_session'); };
  var show = function () {
    row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === 'Suggest A' && !!box(); }, 3000).then(function () { return wait(50); });
  };
  // A fresh panel: another session's and back, so the box holds no state of what it sent before.
  var again = function () {
    var other = all('.ca-lane.is-agent').filter(function (r) { return r !== row(); })[0];
    if (!other) return Promise.reject(new Error('no other session to show between'));
    other.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() !== 'Suggest A'; }, 3000).then(show);
  };

  say('suggest check: waiting for the list');
  until(function () { return !!picked && !!row(); }, 20000, 'a session of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(show)
    .then(function () {
      var chip = row().querySelector('.ca-suggestedchip');
      check('its row has the Suggested chip', !!chip, row().className);
      check('whose tooltip says who suggested what', !!chip && /Suggested by Supervisor/.test(chip.title) && chip.title.indexOf(replies[1]) >= 0, chip ? chip.title : '');
      check('its panel shows them above the box', !!strip() && rows().length === 3 && !!strip().nextElementSibling && strip().nextElementSibling.classList.contains('ca-composer-field'));
      var head = q('.ca-sessinsp .ca-suggest-head');
      check('with who suggested them', !!head && /Suggested by Supervisor/.test(head.textContent), head ? head.textContent : '');
      var n = q('.ca-sessinsp .ca-suggest-note');
      check('and the note', !!n && /asks whether to push/.test(n.textContent), n ? n.textContent : '');
      check('the line under the box says ↑ goes to them', /↑ suggested replies/.test(note()), note());
      type(box(), 'My own draft');
      return wait(50);
    })
    .then(function () {
      rows()[0].click();
      return until(function () { return sends().length === 1; }, 3000, 'a click sending it');
    })
    .then(function () {
      var s = sends()[0];
      check('a click sends it as it is', s.key === picked.key && s.text === replies[0] && !s.now, JSON.stringify(s));
      check('and leaves the draft in the box', box().value === 'My own draft', box().value);
      return until(function () { return !strip(); }, 3000).then(function () { return true; }, function () { return false; });
    })
    .then(function (gone) {
      check('they go once it’s sent', gone);
      return again();
    })
    .then(function () {
      check('a fresh panel shows them again, as the board still has them', !!strip());
      rows()[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, altKey: true }));
      return wait(50);
    })
    .then(function () {
      check('⌥-click puts it in the box after the draft', box().value === 'My own draft\n\n' + replies[1], JSON.stringify(box().value));
      check('and sends nothing', sends().length === 1, sends().length + ' sends');
      type(box(), '');
      return wait(50);
    })
    .then(function () {
      key(box(), { key: 'ArrowUp' });
      return wait(50);
    })
    .then(function () {
      check('↑ in the empty box goes to the nearest one', document.activeElement === rows()[2], document.activeElement ? document.activeElement.className : 'nothing');
      key(rows()[2], { key: 'ArrowUp' });
      check('↑ there goes to the one above', document.activeElement === rows()[1]);
      key(rows()[1], { key: 'Escape' });
      check('Escape goes back to the box', document.activeElement === box());
      key(box(), { key: 'ArrowUp' });
      key(rows()[2], { key: 'Enter' });
      return until(function () { return sends().length === 2; }, 3000, '⏎ sending it');
    })
    .then(function () {
      check('⏎ on one sends it', sends()[1].text === replies[2], JSON.stringify(sends()[1]));
      return again();
    })
    .then(function () { return menuOf(rows()[0]); })
    .then(function (labels) {
      check('a reply’s menu has Send, Change before sending and Dismiss', labels.indexOf('Send') >= 0 && labels.indexOf('Change before sending') >= 0 && labels.indexOf('Dismiss suggestions') >= 0, labels.join(' | '));
      q('.ca-sessinsp .ca-suggest-head .ca-linkbtn').click();
      return until(function () { return sent('dismiss_suggestions').length === 1; }, 3000, 'Dismiss asking the engine');
    })
    .then(function () {
      check('Dismiss asks the engine to take them away', last('dismiss_suggestions').key === picked.key, JSON.stringify(last('dismiss_suggestions')));
      check('and they go at once', !strip());
      check('and the box has the keyboard', document.activeElement === box());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
