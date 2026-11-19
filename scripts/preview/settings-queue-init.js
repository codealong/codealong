// Opens Settings › Queue with due todos first, reviews after the sessions that need you and the
// todos ready to start left out, for a screenshot (preview.sh's sessions view). config.toml reads
// back as it was saved.
//   PREVIEW_INIT=scripts/preview/settings-queue-init.js PREVIEW_NAME=settings-queue scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-queue PREVIEW_INIT=scripts/preview/settings-queue-init.js scripts/preview.sh sessions
(function () {
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      if (!latest) latest = Object.assign({}, view.config, { queue: { order: ['due', 'needs', 'reviews', 'unread'], due_within: 1 } });
      return Object.assign({}, view, { config: latest });
    });
  };
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var nav = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-nav button'), function (b) { return b.textContent.trim() === 'Queue'; })[0];
    if (document.querySelector('.ca-queue-kinds')) {
      clearInterval(open);
    } else if (nav) {
      nav.click();
    } else if (gear) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
