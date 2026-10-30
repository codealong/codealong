// The quick panel folded to the chip that holds a prompt (Quick.tsx `QuickFold`), for a
// screenshot: a prompt typed, then folded as Codealong folds it when another window takes the
// keyboard.
//
//   PREVIEW_INIT=scripts/preview/quick-fold-init.js PREVIEW_NAME=quick-fold scripts/preview.sh quick
(function () {
  var tries = 0;
  var fold = function () {
    var input = document.querySelector('.ca-quick-input textarea');
    if (!input || !window.__quickFold) {
      if (++tries < 300) setTimeout(fold, 100);
      return;
    }
    var set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    set.call(input, 'Fix the pangolin pen so the gate shuts when the latch is down\nand say why it didn’t');
    input.dispatchEvent(new Event('input', { bubbles: true }));
    setTimeout(function () { window.__quickFold(); }, 100);
  };
  fold();
})();
