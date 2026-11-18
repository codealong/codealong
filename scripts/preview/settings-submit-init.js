// Opens Settings › Automation with reviews on, what's submitted for you picked and nudges kept to
// feedback, scrolled to the reviews, for a screenshot of the two selects (preview.sh's sessions
// view).
//   PREVIEW_INIT=scripts/preview/settings-submit-init.js PREVIEW_NAME=settings-submit scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-submit PREVIEW_INIT=scripts/preview/settings-submit-init.js scripts/preview.sh sessions
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      var c = view.config;
      c.reviews = Object.assign({ account: '', model: '', prompt: '', post: 'draft' }, c.reviews, { auto: true, submit: 'approve-or-comment' });
      c.pull_requests = Object.assign({ nudge_by_default: true }, c.pull_requests, { nudge_about: 'feedback' });
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
      var field = Array.prototype.filter.call(document.querySelectorAll('.ca-field > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Where each review goes'; })[0];
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
