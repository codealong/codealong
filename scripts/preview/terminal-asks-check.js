// Checks a session whose command stopped at a question in a terminal under it (terminal.rs
// `overlay_input`, terminals.rs `Glance::asks`): its row is flagged Needs input, its panel opens
// the drawer on the terminal that asks, whose tab says so with the question in its tooltip, and the
// session's box still writes to the session, with no Allow or Deny, since the answer is typed in
// the terminal.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/terminal-asks-check.js
// As PREVIEW_INIT, after a line setting `window.__terminal_screenshot = true`, it stops once the
// drawer shows, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var question = 'Restore the atlas clone? [y/N]';
  var command = './scripts/restore.sh';
  var asking = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if ((picked && picked.key !== s.key) || l.hidden || l.device || s.visitor || s.archived) return;
          picked = picked || { key: s.key, lane: l.id };
          s.hosted = false;
          s.asks = false;
          s.running = { terminal: 'Codealong', command: command, yours: true };
          if (asking) {
            s.status = 'needs';
            s.need = 'input';
            s.statusSince = Date.now() - 20e3;
            s.excerpt = 'Waiting for input · `' + question + '`';
          } else {
            s.status = 'working';
            s.need = undefined;
            s.statusSince = Date.now() - 60e3;
            s.excerpt = 'Waiting on your command · `' + command + '`';
          }
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var drawer = function () { return document.querySelector('.ca-sessinsp .ca-termdrawer'); };
  var tabs = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-termtab')); };
  var term = null;

  say('terminal asks check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return document.querySelector('.ca-sessinsp[data-shows-session="' + picked.key + '"]'); }, 5000);
    })
    .then(function () {
      // The run's terminal, then a shell beside it, which the drawer shows last; folded, so the
      // question has to open it.
      return window.__TAURI_INTERNALS__.invoke('terminal_open', { key: picked.key, folder: '/Users/me/projects/northwind-app', command: command });
    })
    .then(function (g) {
      term = g;
      return window.__TAURI_INTERNALS__.invoke('terminal_open', { key: picked.key, folder: '/Users/me/projects/northwind-app' });
    })
    .then(function () {
      return until(function () { return tabs().length === 2; }, 5000);
    })
    .then(function () {
      var fold = document.querySelector('.ca-sessinsp .ca-termdrawer-act[aria-label="Fold the terminals"]');
      if (fold) fold.click();
      return until(function () { return drawer() && !drawer().classList.contains('is-open'); }, 3000);
    })
    .then(function () {
      check('not flagged before it asks', !row().querySelector('.ca-insight') || row().textContent.indexOf('Needs input') < 0, row().textContent);
      asking = true;
      window.__terminals[picked.key][0] = Object.assign({}, window.__terminals[picked.key][0], { asks: question, asksSince: Date.now() - 20e3 });
      return window.__pushSides();
    })
    .then(function () {
      return until(function () { return row().textContent.indexOf('Needs input') >= 0 && drawer() && drawer().classList.contains('is-open'); }, 5000);
    })
    .then(function () {
      var flag = Array.prototype.filter.call(row().querySelectorAll('.ca-insight'), function (c) { return c.textContent === 'Needs input'; })[0];
      check('the row is flagged Needs input', !!flag && flag.classList.contains('is-warn'), row().textContent);
      check('the flag says where to answer', flag.getAttribute('title').indexOf('type something there') >= 0, flag.getAttribute('title'));
      var active = document.querySelector('.ca-sessinsp .ca-termtab.is-active');
      var asks = tabs()[0];
      check('the drawer opens on the terminal that asks', active === asks, active && active.textContent);
      var state = asks.querySelector('.ca-termtab-state');
      check('its tab says it needs input', !!state && state.textContent === 'needs input' && state.classList.contains('is-asks'), state && state.outerHTML);
      var name = asks.querySelector('.ca-termtab-name');
      check('its tooltip says the question', name.getAttribute('title').indexOf(question) >= 0, name.getAttribute('title'));
      check('the other tab says nothing of it', !tabs()[1].querySelector('.ca-termtab-state.is-asks'));
      check('no Allow or Deny: the answer goes in the terminal', !document.querySelector('.ca-sessinsp .ca-composer.is-approval'));
      check('the box still writes to the session', !!document.querySelector('.ca-sessinsp textarea'));
      check('the keyboard stays where it was', !document.activeElement || !document.activeElement.closest('.ca-term'), document.activeElement && document.activeElement.className);
      if (window.__terminal_screenshot) return;
      // Folded while it goes on asking the same, it stays folded.
      document.querySelector('.ca-sessinsp .ca-termdrawer-act[aria-label="Fold the terminals"]').click();
      return window.__pushSides()
        .then(function () { return wait(300); })
        .then(function () {
          check('folded, it stays folded while it asks the same', !drawer().classList.contains('is-open'));
          // Answered: it works on again.
          asking = false;
          window.__terminals[picked.key][0] = Object.assign({}, window.__terminals[picked.key][0], { asks: undefined, asksSince: undefined });
          return window.__pushSides();
        })
        .then(function () {
          return until(function () { return row().textContent.indexOf('Needs input') < 0; }, 5000);
        })
        .then(function () {
          check('answered, its tab says so no more', !tabs()[0].querySelector('.ca-termtab-state.is-asks'));
        });
    })
    .then(done)
    .catch(stopped);
})();
