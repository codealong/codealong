// Shows the session in the main window's panel as one that runs in a terminal of Codealong's own
// (SessionTerminal.tsx), with the preview stub's sample screen in its terminal, for a screenshot:
//   PREVIEW_INIT=scripts/preview/session-terminal-init.js PREVIEW_NAME=session-terminal scripts/preview.sh sessions
(function () {
  var tries = 0;
  var later = function (f) {
    if (++tries < 400) setTimeout(f, 50);
  };
  var pick = function () {
    var on = Array.from(document.querySelectorAll('.ca-sessview button')).filter(function (b) { return /Terminal/.test(b.textContent); })[0];
    if (on) on.click();
    else later(pick);
  };
  var open = function () {
    var panel = document.querySelector('.ca-sessinsp[data-shows-session]');
    if (!panel) return later(open);
    var key = panel.getAttribute('data-shows-session');
    window.__TAURI_INTERNALS__.invoke('terminal_open', { key: key, folder: '/Users/me/projects/northwind-app', command: null }).then(function (home) {
      window.__terminals[key] = window.__terminals[key].map(function (t) { return t.id === home.id ? Object.assign({}, t, { hosts: true, title: '✳ Claude Code' }) : t; });
      window.__pushSides().then(pick);
    });
  };
  open();
})();
