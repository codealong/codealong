// Opens Set up Codealong (ui/SetupDialog.tsx) over the main window, as the app menu's Set up
// Codealong… does, for a screenshot of its first step:
//   PREVIEW_SETUP=scripts/preview/setup-sample.json PREVIEW_INIT=scripts/preview/setup-init.js PREVIEW_NAME=setup scripts/preview.sh
(function () {
  var open = function () {
    if (document.querySelector('.ca-side-config')) window.__emit('setup', null);
    else setTimeout(open, 50);
  };
  open();
})();
