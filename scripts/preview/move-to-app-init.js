// Opens the Open button's list in the panel of a session that can move to its harness's app (Move
// to Claude app, Move to Codex), for a screenshot of it there, apart from the ways to open it.
// Needs a live terminal session on the account the Claude or Codex app is signed into.
//   PREVIEW_INIT=scripts/preview/move-to-app-init.js PREVIEW_NAME=move-to-app scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var s = null;
  fetch('board.json').then(function (r) { return r.json(); }).then(function (b) {
    var found = null;
    b.lanes.forEach(function (l) {
      l.targets.forEach(function (t) {
        if (!/\|to-app$/.test(t.id) || (found && !found.t.disabled)) return;
        found = { t: t, s: l.sessions.filter(function (x) { return t.id === 's|' + x.key + '|to-app'; })[0] };
      });
    });
    s = found && found.s;
  });
  var step = 0;
  var t = setInterval(function () {
    if (!s) return;
    var row = document.querySelector('.ca-bay-lanes [id^="row-"][id$="' + s.key + '"]');
    var panel = document.querySelector('.ca-sessinsp');
    var caret = panel && panel.textContent.indexOf(s.title) >= 0 && panel.querySelector('[aria-label="Other ways to open"]');
    if (step === 0 && row) {
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      step = 1;
    } else if (step === 1 && caret) {
      caret.click();
      clearInterval(t);
    }
  }, 100);
})();
