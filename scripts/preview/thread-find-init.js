// Opens PREVIEW_TRANSCRIPT's session in Sessions and finds "window" in its conversation (⌘F,
// ThreadFind.tsx), going back one match, for a screenshot of the find bar and its marks.
//   PREVIEW_TRANSCRIPT=<session key> PREVIEW_INIT=scripts/preview/thread-find-init.js scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var wanted = null;
  fetch('./transcript-key.json').then(function (r) { return r.ok ? r.json() : null; }).then(function (k) { wanted = k && k.key; }, function () {});
  var step = 0;
  var t = setInterval(function () {
    if (!wanted) return;
    var row = Array.prototype.slice.call(document.querySelectorAll('[id^="row-"]')).filter(function (r) { return r.id.slice(-wanted.length - 2) === '::' + wanted; })[0];
    var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
    var thread = document.querySelector('.ca-sessinsp .ca-thread');
    var field = document.querySelector('.ca-thread-find input');
    if (step === 0 && row) {
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      step = 1;
    } else if (step === 1 && box && thread && thread.querySelector('.ca-thread-said')) {
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true, bubbles: true, cancelable: true }));
      step = 2;
    } else if (step === 2 && field) {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, 'window');
      field.dispatchEvent(new Event('input', { bubbles: true }));
      step = 3;
    } else if (step >= 3 && step < 6) {
      step++;
    } else if (step === 6 && field) {
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      clearInterval(t);
    }
  }, 100);
})();
