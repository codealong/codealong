// Checks the context meter in a session's reply box (ui/ContextMeter.tsx), on a board whose
// sessions all say their context is 820,093 tokens of a 1,000,000-token window: the meter shows
// 82% at the right of the box's foot, just before the chip that starts with its account, getting
// full; hovering it opens a card above it, ending at its right edge, with the tokens, the
// model and what the session has cost, asked of the ledger for that session; and the side
// conversation's box has none.
//   scripts/preview.sh sessions '' scripts/preview/context-check.js
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
  var meter = function () { return q('.ca-sessinsp .ca-composer .ca-context'); };

  say('context check: opening a session');
  until(function () {
    if (!meter()) {
      var row = q('.ca-row[role="option"]');
      if (row) row.click();
    }
    return meter();
  }, 20000, 'the meter in a session’s box')
    .then(function (m) {
      check('it shows the share of the window', m.textContent.trim() === '82%', m.textContent);
      check('it looks like it’s getting full', m.classList.contains('is-high'), m.className);
      check('it says the tokens to a screen reader', /820K of 1M tokens, 82% of its window/.test(m.getAttribute('aria-label')), m.getAttribute('aria-label'));
      var row = m.closest('.ca-composer-field .ca-composer-buttons');
      var next = m.nextElementSibling;
      check('it sits in the box’s foot, at the right, just before the chip with its account', !!row && !!next && !!q('.ca-account-name', next), row ? row.innerHTML.slice(0, 120) : 'no row');
      var field = m.closest('.ca-composer-field').getBoundingClientRect();
      var send = q('.ca-sessinsp .ca-composer-buttons').getBoundingClientRect();
      var at = m.getBoundingClientRect();
      check('level with the buttons beside it', Math.abs((at.top + at.bottom) / 2 - (send.top + send.bottom) / 2) <= 2, Math.round(at.top) + '–' + Math.round(at.bottom) + ' beside ' + Math.round(send.top) + '–' + Math.round(send.bottom));
      check('and inside the box', at.bottom <= field.bottom && at.left >= field.left, Math.round(at.bottom) + ' in ' + Math.round(field.bottom));
      check('nothing is under the box', !q('.ca-sessinsp .ca-composer:not(.is-approval) > .ca-composer-note'), (q('.ca-sessinsp .ca-composer:not(.is-approval) > .ca-composer-note') || {}).textContent);
      check('it draws a pie', !!m.querySelector('svg.ca-context-pie .ca-context-fill'));
      m.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
      return until(function () { var c = q('.ca-context-card'); return c && /Cost so far/.test(c.textContent) && c; }, 3000, 'its card, with the cost');
    })
    .then(function (card) {
      var c = card.getBoundingClientRect();
      var m = meter().getBoundingClientRect();
      check('the card is above the meter, ending at its right edge', c.bottom <= m.top && Math.abs(c.right - m.right) <= 1, Math.round(c.right) + ',' + Math.round(c.bottom) + ' over ' + Math.round(m.right) + ',' + Math.round(m.top));
      check('the card says the tokens of the window', /820K of 1M tokens/.test(card.textContent), card.textContent);
      check('and what the session has cost', /Cost so far\$[\d,.]+ at API prices/.test(card.textContent), card.textContent);
      var asked = last('session_tokens');
      check('asking the ledger for that session', !!asked && typeof asked.key === 'string' && asked.key.length > 0, JSON.stringify(asked));
      meter().dispatchEvent(new MouseEvent('mouseout', { bubbles: true }));
      return until(function () { return !q('.ca-context-card'); }, 3000, 'the card to go');
    })
    .then(function () {
      check('leaving it puts the card away', !q('.ca-context-card'));
    })
    .then(done)
    .catch(stopped);
})();
