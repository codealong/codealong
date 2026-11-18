// Opens Settings › Experimental with Voice turned on, so its shortcuts and settings show, for a
// screenshot (preview.sh's sessions view). config.toml reads back as it was saved.
//   PREVIEW_INIT=scripts/preview/settings-voice-init.js PREVIEW_NAME=settings-voice scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-voice PREVIEW_INIT=scripts/preview/settings-voice-init.js scripts/preview.sh sessions
(function () {
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      if (!latest) latest = Object.assign({}, view.config, { experimental: Object.assign({}, view.config.experimental, { voice: true }) });
      return Object.assign({}, view, { config: latest });
    });
  };
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var nav = Array.prototype.filter.call(document.querySelectorAll('.ca-settings .ca-nav, .ca-settings-nav button'), function (b) { return b.textContent.trim() === 'Experimental'; })[0];
    var voice = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Voice'; })[0];
    if (voice) clearInterval(open);
    else if (nav) nav.click();
    else if (gear) gear.click();
    if (tries > 200) clearInterval(open);
  }, 50);
})();
