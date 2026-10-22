// Opens Close stale terminals (ui/CloseStaleDialog.tsx) over the main window, as File › Close stale
// terminals… does, for a screenshot of the tabs the preview's stub lists:
//   PREVIEW_INIT=scripts/preview/close-stale-init.js PREVIEW_NAME=close-stale scripts/preview.sh
(function () {
  var open = function () {
    if (document.querySelector('.ca-side-config')) window.__emit('close-stale', null);
    else setTimeout(open, 50);
  };
  open();
})();
