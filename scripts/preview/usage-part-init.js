// Opens the Usage window (ui/UsageDialog.tsx) on Your part, for a screenshot of the analytics
// experiment's views (ui/UsageAnalytics.tsx); usage-sessions-init.js opens it on Sessions:
//   PREVIEW_ANALYTICS=1 PREVIEW_INIT=scripts/preview/usage-part-init.js PREVIEW_NAME=usage-part scripts/preview.sh
//   PREVIEW_ANALYTICS=1 PREVIEW_INIT=scripts/preview/usage-sessions-init.js PREVIEW_NAME=usage-sessions scripts/preview.sh
(function () {
  var view = window.__USAGE_VIEW__ || 'Your part';
  var tries = 0;
  var open = setInterval(function () {
    var button = Array.from(document.querySelectorAll('.ca-usagebar-end button')).find(function (b) { return b.textContent.trim() === 'Usage'; });
    var tab = Array.from(document.querySelectorAll('.ca-usage-filters button')).find(function (b) { return b.textContent.trim() === view; });
    if (tab) {
      clearInterval(open);
      tab.click();
    } else if (button && !document.querySelector('.ca-usage')) {
      button.click();
    } else if (++tries > 200) {
      clearInterval(open);
    }
  }, 50);
})();
