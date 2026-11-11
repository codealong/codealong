// Opens the Usage window on Sessions, for a screenshot (usage-part-init.js says how).
window.__USAGE_VIEW__ = 'Sessions';
(function () {
  var view = 'Sessions';
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
