// Opens the Usage window (ui/UsageDialog.tsx) from the usage bar's Usage button, for a screenshot:
//   PREVIEW_INIT=scripts/preview/usage-init.js PREVIEW_NAME=usage scripts/preview.sh
(function () {
  var tries = 0;
  var open = setInterval(function () {
    var button = Array.from(document.querySelectorAll('.ca-usagebar-end button')).find(function (b) { return b.textContent.trim() === 'Usage'; });
    if (button || ++tries > 100) {
      clearInterval(open);
      if (button) button.click();
    }
  }, 50);
})();
