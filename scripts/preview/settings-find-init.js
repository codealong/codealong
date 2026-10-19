// Opens Settings and searches it for "review" (SettingsFind.tsx), for a screenshot (preview.sh's
// sessions view).
//   PREVIEW_INIT=scripts/preview/settings-find-init.js PREVIEW_NAME=settings-find scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_NAME=settings-find PREVIEW_INIT=scripts/preview/settings-find-init.js scripts/preview.sh sessions
(function () {
  var words = 'review';
  var tries = 0;
  var open = setInterval(function () {
    tries += 1;
    var gear = document.querySelector('.ca-side-config');
    var field = document.querySelector('.ca-settings-find input');
    if (field && document.querySelector('.ca-settings-pane .ca-field')) {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, words);
      field.dispatchEvent(new Event('input', { bubbles: true }));
      clearInterval(open);
    } else if (gear && !field) {
      gear.click();
    }
    if (tries > 200) clearInterval(open);
  }, 50);
})();
