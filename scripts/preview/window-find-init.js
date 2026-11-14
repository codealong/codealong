// Opens the window's find (⌘F, ui/WindowFind.tsx) and finds PREVIEW_FIND's words (default
// "session") in whatever the view shows, stepping on once, for a screenshot of the bar and its
// marks.
//   PREVIEW_INIT=scripts/preview/window-find-init.js scripts/preview.sh schedules
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var words = window.__PREVIEW_FIND__ || 'session';
  var step = 0;
  var t = setInterval(function () {
    var field = document.querySelector('.ca-findbar input');
    if (step === 0 && document.querySelector('.ca-board, .ca-peek, .ca-waiting')) {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', code: 'KeyF', metaKey: true, bubbles: true, cancelable: true }));
      step = 1;
    } else if (step === 1 && field) {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, words);
      field.dispatchEvent(new Event('input', { bubbles: true }));
      step = 2;
    } else if (step >= 2 && step < 5) {
      step++;
    } else if (step === 5 && field) {
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      clearInterval(t);
    }
  }, 100);
})();
