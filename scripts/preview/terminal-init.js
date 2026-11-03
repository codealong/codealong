// Opens terminals of Codealong's own under the session shown in the main window's panel, for a
// screenshot of the drawer with the preview stub's sample screen: a command that ran and failed,
// as Run leaves one, then a shell from the drawer's +, which it shows with the keyboard:
//   PREVIEW_INIT=scripts/preview/terminal-init.js PREVIEW_NAME=terminal scripts/preview.sh sessions
(function () {
  var tries = 0;
  var later = function (f) {
    if (++tries < 400) setTimeout(f, 50);
  };
  var plus = function () {
    var add = document.querySelector('.ca-termdrawer-act[aria-label="New terminal"]');
    if (add) add.click();
    else later(plus);
  };
  var open = function () {
    var panel = document.querySelector('.ca-sessinsp[data-shows-session]');
    if (!panel) return later(open);
    var key = panel.getAttribute('data-shows-session');
    window.__TAURI_INTERNALS__.invoke('terminal_open', { key: key, folder: '/Users/me/projects/northwind-app', command: 'npm run lint' }).then(function (ran) {
      window.__terminals[key][0] = Object.assign({}, ran, { running: false, exit: 1 });
      plus();
    });
  };
  open();
})();
