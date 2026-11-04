// Checks a session that runs in a terminal of Codealong's own (`[apps] terminal = "codealong"`,
// launch/codealong.rs; ui/terminal/SessionTerminal.tsx): its panel has a switch between its
// conversation and its terminal, on the conversation at first, and the drawer under its box
// doesn't list that terminal; Terminal shows it in the conversation's place, with the keyboard,
// the box gone; ⌃` there goes back to the conversation and its box; the server asking to show it
// (a reveal of kind `terminal`, as Show in does) shows it again; and a terminal that's no
// session's (a folder opened in Codealong's terminal) gets a window of its own instead.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/session-terminal-check.js
(function () {
  (async function () {
    try {
      var panel = await until(function () { return q('.ca-sessinsp[data-shows-session]'); }, 30000, 'a session in the panel');
      var key = panel.getAttribute('data-shows-session');
      check('no switch for a session that runs in no terminal of Codealong’s', !q('.ca-sessview', panel));
      // The session's own terminal, as the engine marks one its process runs in (`adopt`), and a
      // shell beside it in its drawer.
      var home = await window.__TAURI_INTERNALS__.invoke('terminal_open', { key: key, folder: '/Users/me/projects/northwind-app', command: null });
      var beside = await window.__TAURI_INTERNALS__.invoke('terminal_open', { key: key, folder: '/Users/me/projects/northwind-app', command: null });
      window.__terminals[key] = window.__terminals[key].map(function (t) { return t.id === home.id ? Object.assign({}, t, { hosts: true, title: '✳ Claude Code' }) : t; });
      await window.__pushSides();
      var view = await until(function () { return q('.ca-sessview', panel); }, 5000, 'the switch');
      var picked = function () { var on = q('.ca-sessview [aria-checked="true"]', panel); return on ? on.textContent.trim() : ''; };
      check('a switch between its conversation and its terminal, on the conversation', /Conversation/.test(picked()), picked());
      check('its conversation and box show', !!q('.ca-composer-field textarea', panel) && !q('.ca-sessterm', panel));
      var tabs = all('.ca-termdrawer .ca-termtab', panel);
      check('the drawer lists the terminal beside it, not the one it runs in', tabs.length === 1 && !/Claude Code/.test(tabs[0].textContent), tabs.map(function (t) { return t.textContent; }).join(' | '));

      all('button', view).filter(function (b) { return /Terminal/.test(b.textContent); })[0].click();
      var field = await until(function () { return q('.ca-sessterm .ca-term-field', panel); }, 5000, 'its terminal in the panel');
      await until(function () { return document.activeElement === field; }, 3000, 'the terminal with the keyboard');
      check('Terminal shows it in the conversation’s place, with the keyboard', !q('.ca-thread', panel) && document.activeElement === field);
      check('the box is gone: what you type goes to the terminal', !q('.ca-composer-field textarea', panel));
      await until(function () { return sent('terminal_watch').some(function (w) { return w.id === home.id && w.on; }); }, 3000, 'the window watching it');
      check('the window watches the terminal it shows', true);

      key0(field);
      await until(function () { return q('.ca-composer-field textarea', panel) === document.activeElement; }, 3000, 'the box with the keyboard');
      check('⌃` in its terminal goes back to the conversation and its box', !q('.ca-sessterm', panel) && /Conversation/.test(picked()));

      // Show in, as the server asks it of a session in Codealong's terminal.
      window.__emit('reveal', { kind: 'terminal', id: home.id });
      await until(function () { return q('.ca-sessterm .ca-term-field', panel); }, 5000, 'its terminal shown again');
      check('the server asking to show its terminal shows it', /Terminal/.test(picked()));

      // A folder opened in Codealong's terminal: no session's, so a window of its own.
      var loose = await window.__TAURI_INTERNALS__.invoke('terminal_open', { key: '', folder: '/Users/me/projects/atlas', command: null });
      var windows = calls('terminal_window').length;
      window.__emit('reveal', { kind: 'terminal', id: loose.id });
      await until(function () { return calls('terminal_window').length > windows; }, 5000, 'a window for it');
      var asked = last('terminal_window');
      check('a terminal that’s no session’s gets a window of its own', asked.id === loose.id && asked.key === '', JSON.stringify(asked));
      done();
    } catch (e) {
      stopped(e);
    }
  })();

  function key0(el) {
    key(el, { key: '`', code: 'Backquote', ctrlKey: true });
  }
})();
