// Opens Settings › Decisions with TypeSafe's key saved, for a screenshot of its usage (fake-tauri.js's made-up
// report) above the features (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/jev-usage-init.js PREVIEW_NAME=jev-usage scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/jev-usage-init.js PREVIEW_NAME=jev-usage scripts/preview.sh sessions
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'decision_keys') return Promise.resolve({ typesafe: true });
    return real(cmd, args, options);
  };
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var nav = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-nav button'), function (b) { return b.textContent.trim() === 'Decisions'; })[0];
    if (nav) {
      nav.click();
      clearInterval(open);
    } else if (gear) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
