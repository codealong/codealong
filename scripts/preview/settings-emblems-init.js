// Opens Settings › General at Session emblems with Drawn picked and a prompt of your own, for a
// screenshot (preview.sh's sessions view). config.toml reads back as it was saved.
//   PREVIEW_INIT=scripts/preview/settings-emblems-init.js PREVIEW_NAME=settings-emblems scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-emblems PREVIEW_INIT=scripts/preview/settings-emblems-init.js scripts/preview.sh sessions
(function () {
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      if (!latest) {
        latest = Object.assign({}, view.config, { emblems: { kind: 'drawn', account: '', prompt: 'Pixel art on a 16 by 16 grid, with a dark outline.' } });
      }
      return Object.assign({}, view, { config: latest });
    });
  };
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var field = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Session emblems'; })[0];
    if (field) {
      field.scrollIntoView({ block: 'start' });
      clearInterval(open);
    } else if (gear) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
