// Types a few words into the quick panel, as a search for a session is typed, for a screenshot of
// what Jev suggests for them (route-find-sample.json answers them as a search).
//   PREVIEW_ROUTE=scripts/preview/route-find-sample.json PREVIEW_INIT=scripts/preview/quick-search-init.js PREVIEW_NAME=quick-search scripts/preview.sh quick
(function () {
  var words = 'release notes draft';
  var t = setInterval(function () {
    var box = document.querySelector('.ca-quick-input textarea');
    if (!box || !document.querySelector('.ca-quick-compose')) return;
    clearInterval(t);
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, words);
    box.dispatchEvent(new Event('input', { bubbles: true }));
  }, 100);
})();
