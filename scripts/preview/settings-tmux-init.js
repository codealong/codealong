// Opens Settings › General scrolled to the terminal settings, Run sessions in tmux among them,
// for a screenshot (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-tmux-init.js PREVIEW_NAME=settings-tmux scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-tmux PREVIEW_INIT=scripts/preview/settings-tmux-init.js scripts/preview.sh sessions
(function () {
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var field = Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field > span:not(.ca-checkbox)'), function (s) { return s.textContent === 'Terminal'; })[0];
    if (field) {
      field.scrollIntoView({ block: 'start' });
      clearInterval(open);
    } else if (gear) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
