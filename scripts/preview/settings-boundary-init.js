// Opens Settings › Routing on its routing groups, for a screenshot of a boundary group's card
// (boundary.rs): its Boundary tag, and a line for each member whose sign-in doesn't qualify.
// The snapshot needs a boundary group and the board's sign-ins, which a Mac may not have: make
// them with PREVIEW_PREPARE=only first, then edit .preview/config.json and board.json.
//   PREVIEW_INIT=scripts/preview/settings-boundary-init.js PREVIEW_NAME=settings-boundary PREVIEW_PREPARE=skip scripts/preview.sh sessions
(function () {
  var button = function (label) { return Array.prototype.filter.call(document.querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var go = setInterval(function () {
    if (step === 0 && document.querySelector('.ca-usagebar-settings')) {
      document.querySelector('.ca-usagebar-settings').click();
      step = 1;
    } else if (step === 1 && button('Routing')) {
      button('Routing').click();
      step = 2;
    } else if (step === 2) {
      var tag = Array.prototype.filter.call(document.querySelectorAll('.ca-tag'), function (t) { return t.textContent.trim() === 'Boundary'; })[0];
      if (!tag) return;
      tag.closest('.ca-ucard').scrollIntoView({ block: 'center' });
      clearInterval(go);
    }
  }, 100);
})();
