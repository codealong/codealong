// Checks a session's terminals of Codealong's own in a card, here the notch's (SessionCard.tsx,
// SessionPanel.tsx, terminal/TerminalDrawer.tsx): no drawer before there's a terminal; one that
// opens for the session while the card shows it (as Run on a command's does, the server opening
// it under the session) shows itself in a drawer under the card's box, leaving the keyboard
// where it was, the window watching it at its size; the drawer takes at most half the card; ⌃`
// from the box gives the terminal the keyboard; Escape there is the program's and leaves the
// notch open; ⌃` hides the drawer and gives the box the keyboard back.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/card-terminal-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  (async function () {
    try {
      // Open the notch on a session, as a click on the notch does (a click on that session's pill
      // again would put it away).
      var shape = await until(function () { return q('.ca-notch-count') && q('.ca-notch-shape'); }, 30000, 'the notch with sessions on its bar');
      shape.click();
      var card = await until(function () { return notch().classList.contains('is-session') && q('.ca-notch-session [data-shows-session]'); }, 10000, 'the notch on a session');
      var sessionKey = card.getAttribute('data-shows-session');
      if (sessionKey.charAt(0) === '@') throw new Error('the notch opened on another machine’s session, which has no terminals here');
      var box = await until(function () { return q('.ca-composer-field textarea', card); }, 10000, 'the session’s box');
      check('no drawer before there’s a terminal', !q('.ca-termdrawer', card));

      // A terminal opens for the session, as the server opens one for Run on a command.
      box.focus();
      window.__TAURI_INTERNALS__.invoke('terminal_open', { key: sessionKey, folder: '/Users/me/projects/northwind-app', command: 'npm test' });
      var drawer = await until(function () { return q('.ca-termdrawer.is-open', card); }, 5000, 'the drawer open in the card');
      var tabs = all('.ca-termtab', drawer);
      check('it shows itself in a drawer under the card’s box, with a tab for it', tabs.length === 1 && drawer.parentElement === card && !!q(':scope > .ca-composer', card) && (q(':scope > .ca-composer', card).compareDocumentPosition(drawer) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0, tabs.map(function (t) { return t.textContent; }).join(' | '));
      check('leaving the keyboard in the box', document.activeElement === box, document.activeElement && document.activeElement.className);
      await until(function () { return q('.ca-term-canvas', drawer) && sent('terminal_watch').some(function (w) { return w.on; }); }, 3000, 'the terminal drawn and watched');
      await until(function () { return sent('terminal_resize').length; }, 3000, 'its size given');
      var size = last('terminal_resize').size;
      check('the window says it shows it, at its size in whole cells', size.cols > 10 && size.rows > 2, JSON.stringify(size));
      var cardTall = card.getBoundingClientRect().height;
      var drawerTall = drawer.getBoundingClientRect().height;
      check('the drawer takes at most half the card', drawerTall <= cardTall / 2 + 1, Math.round(drawerTall) + ' of ' + Math.round(cardTall));
      var canvas = q('.ca-term-canvas', drawer);
      var screen = q('.ca-term-screen', drawer);
      check('the terminal fits the card', canvas.getBoundingClientRect().width <= screen.getBoundingClientRect().width + 1, Math.round(canvas.getBoundingClientRect().width) + ' in ' + Math.round(screen.getBoundingClientRect().width));

      // ⌃` from the box gives the terminal the keyboard.
      key(box, { key: '`', code: 'Backquote', ctrlKey: true });
      var field = await until(function () { return q('.ca-term-field', card); }, 3000, 'the terminal');
      await until(function () { return document.activeElement === field; }, 3000, 'the terminal with the keyboard');
      check('⌃` from the box gives the terminal the keyboard, opening no other', document.activeElement === field && calls('terminal_open').length === 1);

      // Escape is the program's: it reaches the terminal and the notch stays open.
      var before = calls('terminal_input').length;
      key(field, { key: 'Escape', code: 'Escape' });
      await until(function () { return sent('terminal_input').slice(before).some(function (a) { return a.inputs.some(function (i) { return i.code === 'Escape'; }); }); }, 3000, 'Escape sent to the terminal');
      await wait(300);
      check('Escape in the terminal goes to its program, and the notch stays open', notch().classList.contains('is-session') && !!q('.ca-notch-session [data-shows-session]'));

      // ⌃` there hides the drawer and gives the box the keyboard back.
      key(field, { key: '`', code: 'Backquote', ctrlKey: true });
      await until(function () { return !q('.ca-termdrawer.is-open', card); }, 3000, 'the drawer folded');
      await until(function () { return document.activeElement === q('.ca-composer-field textarea', card); }, 3000, 'the box with the keyboard');
      check('⌃` in the terminal folds the drawer to its tab and gives the box the keyboard back', !!q('.ca-termdrawer .ca-termtab', card) && document.activeElement === q('.ca-composer-field textarea', card));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
