// Opens Settings › Automation with reviews on, and reviews of your own pull requests on with Cursor
// named as their reviewer, scrolled to Where reviews run, for a screenshot of the select and of the
// line that says Cursor can't start a review without a terminal (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-review-start-init.js PREVIEW_NAME=settings-review-start scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-review-start PREVIEW_INIT=scripts/preview/settings-review-start-init.js scripts/preview.sh sessions
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  // What Settings saved is what it reads back, as the file would have it.
  var saved = null;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') saved = JSON.parse(JSON.stringify(args.config.reviews));
    var p = real(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      var c = view.config;
      if (saved) {
        c.reviews = saved;
        return view;
      }
      // Where reviews run as a file from before it has it: never in a terminal.
      c.reviews = Object.assign({ account: '', model: '', prompt: '', post: 'draft', submit: 'never' }, c.reviews, { auto: true });
      delete c.reviews.start;
      c.reviews.own = Object.assign({ model: '', prompt: '', send: true }, c.reviews.own, { auto: true, account: 'cursor' });
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
      var field = Array.prototype.filter.call(document.querySelectorAll('.ca-field > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Where reviews run'; })[0];
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
