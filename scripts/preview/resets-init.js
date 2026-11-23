// The usage bar with a Codex account holding three limit resets and its card up, for a
// screenshot of the chip's mark and the card.
//   PREVIEW_RESETS=scripts/preview/resets-sample.json PREVIEW_INIT=scripts/preview/resets-init.js PREVIEW_NAME=resets scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_RESETS=scripts/preview/resets-sample.json PREVIEW_INIT=scripts/preview/resets-init.js PREVIEW_NAME=resets scripts/preview.sh sessions
(function () {
  var tries = 0;
  var hover = function () {
    var chip = Array.prototype.filter.call(document.querySelectorAll('.ca-statusbar-accts .ca-uchip'), function (c) {
      return c.querySelector('.ca-uchip-resets');
    })[0];
    if (!chip) {
      if (tries++ < 80) setTimeout(hover, 100);
      return;
    }
    chip.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
  };
  setTimeout(hover, 300);
})();
