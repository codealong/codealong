// A terminal of Codealong's own saying for a moment that ⌘C didn't copy, for a screenshot of the
// note over the terminal's corner (TerminalView.tsx `said`): the selection read, the clipboard
// refusing it. The key's pressed late in the screenshot's virtual time, so the note's still up.
//   PREVIEW_INIT=scripts/preview/term-note-init.js PREVIEW_NAME=term-note scripts/preview.sh terminal
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/term-note-init.js PREVIEW_NAME=term-note scripts/preview.sh terminal
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'terminal_copy') return Promise.resolve('npm run lint');
    return real(cmd, args, options);
  };
  Clipboard.prototype.write = function () {
    return Promise.reject(new DOMException('The request is not allowed by the user agent', 'NotAllowedError'));
  };
  var tries = 0;
  var run = function () {
    var field = document.querySelector('.ca-term-field');
    if ((!field || performance.now() < 3500) && ++tries < 400) return setTimeout(run, 50);
    if (!field) return;
    field.focus();
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', code: 'KeyC', metaKey: true, bubbles: true, cancelable: true }));
  };
  run();
})();
