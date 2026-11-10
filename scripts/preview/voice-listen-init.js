// Turns voice on and opens the dialog that sets a session's listening up, as Listen… in the
// session's menu does, for the first session listed, for a screenshot of it (preview.sh's
// sessions view). `window.__LISTEN_ROLE__` picks what it's asked to do first (`requests`, say).
//   PREVIEW_NAME=voice-listen PREVIEW_INIT=scripts/preview/voice-listen-init.js scripts/preview.sh sessions
(function () {
  window.__PREVIEW_VOICE__ = true;
  var t = setInterval(function () {
    var row = document.querySelector('.ca-board-list [id^="row-"][id*="::"]');
    if (!row) return;
    clearInterval(t);
    var at = row.id.indexOf('::');
    var title = row.querySelector('.ca-agentline-title, .ca-lane-title');
    window.dispatchEvent(new CustomEvent('codealong-listen', { detail: { key: row.id.slice(at + 2), laneId: row.id.slice(4, at), title: title ? title.textContent : 'This session' } }));
    var role = window.__LISTEN_ROLE__;
    if (!role) return;
    var picked = setInterval(function () {
      var seg = Array.prototype.slice.call(document.querySelectorAll('.ca-listendlg .ca-segctl button')).filter(function (b) { return b.textContent.trim().toLowerCase() === role; })[0];
      if (!seg) return;
      clearInterval(picked);
      seg.click();
    }, 60);
  }, 60);
})();
