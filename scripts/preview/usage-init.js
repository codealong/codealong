// Opens the Analytics window (ui/UsageDialog.tsx) from the sidebar's Analytics button, for a screenshot:
//   PREVIEW_INIT=scripts/preview/usage-init.js PREVIEW_NAME=usage scripts/preview.sh
(function () {
  var tries = 0;
  var open = setInterval(function () {
    var button = document.querySelector('.ca-side-analytics, .ca-rail-btn[aria-label^="Analytics"]');
    if (button || ++tries > 100) {
      clearInterval(open);
      if (button) button.click();
    }
  }, 50);
})();
