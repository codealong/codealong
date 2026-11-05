// Opens a terminal of Codealong's own under the session a card shows, for a screenshot of the
// drawer in a card: in the notch (opened on a session first) or the peek, a command that ran and
// failed, as Run on a command leaves one, the server having opened it under the session.
//   PREVIEW_SIZE=1512,980 PREVIEW_INIT=scripts/preview/card-terminal-init.js PREVIEW_NAME=card-terminal scripts/preview.sh notch
//   PREVIEW_INIT=scripts/preview/card-terminal-init.js PREVIEW_NAME=card-terminal-peek scripts/preview.sh peek
(function () {
  var peek = !!window.__CODEALONG_PEEK__;
  var tries = 0;
  var later = function (f) {
    if (++tries < 400) setTimeout(f, 50);
  };
  var opened = false;
  var run = function () {
    if (!peek) {
      var shape = document.querySelector('.ca-notch-shape');
      if (!shape || !document.querySelector('.ca-notch-count')) return later(run);
      if (!opened) {
        opened = true;
        shape.click();
        return later(run);
      }
    }
    var card = document.querySelector(peek ? '.ca-peek [data-shows-session]' : '.ca-notch.is-session .ca-notch-session [data-shows-session]');
    if (!card) return later(run);
    var key = card.getAttribute('data-shows-session');
    window.__TAURI_INTERNALS__.invoke('terminal_open', { key: key, folder: '/Users/me/projects/northwind-app', command: 'npm run lint' }).then(function (ran) {
      window.__terminals[key][0] = Object.assign({}, ran, { running: false, exit: 1 });
    });
  };
  run();
})();
