// Opens Settings › Automation scrolled to resuming a stopped session, for a screenshot of the
// resume settings, the one for a pull request's news last (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-resume-init.js PREVIEW_NAME=settings-resume scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-resume PREVIEW_INIT=scripts/preview/settings-resume-init.js scripts/preview.sh sessions
(function () {
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var nav = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-nav button'), function (b) { return b.textContent.trim() === 'Automation'; })[0];
    if (nav) {
      nav.click();
      var field = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-check > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Resume a stopped session for another session’s message'; })[0];
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
