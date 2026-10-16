// Opens the list's find (⌘F from the list, ListBar.tsx) and types "co" in it, for a screenshot of
// the find over a view's list as it narrows it.
//   PREVIEW_INIT=scripts/preview/list-find-init.js scripts/preview.sh sessions
//   PREVIEW_INIT=scripts/preview/list-find-init.js scripts/preview.sh canvas
(function () {
  var words = 'co';
  var t = setInterval(function () {
    var board = document.querySelector('.ca-board');
    if (!board || document.querySelector('.ca-board-loading')) return;
    var field = document.querySelector('.ca-listfind input');
    if (!field) {
      board.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', code: 'KeyF', metaKey: true, bubbles: true, cancelable: true }));
      return;
    }
    clearInterval(t);
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, words);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }, 100);
})();
