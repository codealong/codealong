// Opens Settings › Automation with nudges on, kept to feedback and checks with a merge too,
// scrolled to them, for a screenshot of what a nudge tells of and its merge box (preview.sh's
// sessions view).
//   PREVIEW_INIT=scripts/preview/settings-nudge-init.js PREVIEW_NAME=settings-nudge scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-nudge PREVIEW_INIT=scripts/preview/settings-nudge-init.js scripts/preview.sh sessions
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      view.config.pull_requests = Object.assign({}, view.config.pull_requests, { nudge_by_default: true, nudge_about: 'feedback-checks-and-merges' });
      return view;
    });
  };
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var nav = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-nav button'), function (b) { return b.textContent.trim() === 'Automation'; })[0];
    if (nav) {
      nav.click();
      var field = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-check > span:not(.ca-checkbox)'), function (s) { return s.textContent.indexOf('Nudge a pull request’s primary session') === 0; })[0];
      if (field) {
        field.scrollIntoView({ block: 'start' });
        clearInterval(open);
      }
    } else if (gear) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
