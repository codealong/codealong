// Right-clicks the first session (or card) in a view and notes its menu's items, to see that a
// session's menu reads the same in every window (menus.ts):
//   PREVIEW_WAIT=20000 scripts/preview.sh sessions '' scripts/preview/menus-check.js
//   PREVIEW_WAIT=20000 scripts/preview.sh bar '' scripts/preview/menus-check.js
//   PREVIEW_WAIT=20000 scripts/preview.sh peek '' scripts/preview/menus-check.js
//   PREVIEW_WAIT=20000 scripts/preview.sh quick '' scripts/preview/menus-check.js
//   PREVIEW_WAIT=20000 scripts/preview.sh desktop '' scripts/preview/menus-check.js
// The peek also drops its menu from the header's ⋯ button, which should list the same items.
(function () {
  var targets = ['.ca-hud [data-key]', '.ca-reply .ca-sessinsp-head', '.ca-quick-scroll [role="option"]', '.ca-desk', '.ca-bay-lanes > *'];
  var labels = function () { return menuLabels().join(' | '); };
  say('menus check: waiting');
  until(function () { for (var i = 0; i < targets.length; i++) { var el = document.querySelector(targets[i]); if (el) return el; } return null; }, 15000, 'nothing to right-click')
    // A moment for the data behind it: a desktop card is there before its feed is.
    .then(function (el) { return wait(1500).then(function () { return el; }); })
    .then(function (el) {
      return menuOf(el).then(function () {
        var right = labels();
        check('a right-click gives its menu', right.length > 0);
        note(right);
        var more = document.querySelector('.ca-reply .ca-sessinsp-head [aria-label="More"]');
        if (!more) return;
        var n = menus();
        more.click();
        return until(function () { return menus() > n; }, 3000).then(function () {
          check('⋯ lists the same', labels() === right, labels());
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
