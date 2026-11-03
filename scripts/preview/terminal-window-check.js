// Checks a terminal popped out into a window of its own (ui/terminal/TerminalWindow.tsx): it shows
// the terminal with its name; it stays when its session moves to another account and the terminal
// goes on under the copy's key (terminals.rs `follow`); it follows Codealong's theme; and it
// closes once the board no longer lists the terminal.
//   PREVIEW_WAIT=30000 scripts/preview.sh terminal '' scripts/preview/terminal-window-check.js
(function () {
  (async function () {
    try {
      var canvas = await until(function () { return q('.ca-termwin .ca-term-canvas'); }, 20000, 'the terminal in its window');
      await until(function () { return canvas.dataset.renderer; }, 5000, 'a renderer');
      check('it shows the terminal, drawn', !!canvas.dataset.renderer, canvas.dataset.renderer);
      var name = q('.ca-termwin-name');
      check('with its name', name && name.textContent === 'zsh', name && name.textContent);
      await until(function () { return calls('kept_ready').length; }, 5000, 'the window shown');
      var key = Object.keys(window.__terminals)[0];
      var t = window.__terminals[key][0];
      // Its session moved: the terminal goes on under the copy's key.
      delete window.__terminals[key];
      window.__terminals['codex:codex:the-copy'] = [Object.assign({}, t, { session: 'codex:codex:the-copy' })];
      await window.__pushSides();
      await wait(500);
      check('it stays when its session moves to another account', calls('kept_close').length === 0 && !!q('.ca-termwin .ca-term-canvas'));
      // Codealong's theme, whatever the system's.
      var theme = (await window.__TAURI_INTERNALS__.invoke('get_board')).theme;
      window.__emit('board', Object.assign(await window.__TAURI_INTERNALS__.invoke('get_board'), { theme: 'dark' }));
      await until(function () { return document.documentElement.getAttribute('data-theme') === 'dark'; }, 3000, 'the dark theme');
      check('it follows Codealong’s theme', document.documentElement.getAttribute('data-theme') === 'dark', theme);
      // Closed: the board lists it no longer.
      delete window.__terminals['codex:codex:the-copy'];
      await window.__pushSides();
      await until(function () { return calls('kept_close').length; }, 3000, 'the window closed');
      check('it closes once the terminal has', calls('kept_close').length === 1);
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
