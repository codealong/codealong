// Opens the bar docked in the notch on a session, for a screenshot of it (preview.sh's notch
// view): the session PREVIEW_TRANSCRIPT names when its pill is on the bar, else the first pill.
//   PREVIEW_SIZE=1512,720 PREVIEW_INIT=scripts/preview/notch-session-init.js scripts/preview.sh notch
(function () {
  var wanted = null;
  fetch('./transcript-key.json').then(function (r) { return r.ok ? r.json() : null; }).then(function (k) { wanted = k && k.key; }, function () {});
  var opened = false;
  var t = setInterval(function () {
    var shape = document.querySelector('.ca-notch-shape');
    if (!shape || !document.querySelector('.ca-notch-count')) return;
    if (!opened) {
      opened = true;
      shape.click();
      return;
    }
    var pills = Array.prototype.slice.call(document.querySelectorAll('.ca-notch-body .ca-hud-pill[data-key]'));
    if (pills.length === 0) return;
    var pill = pills.filter(function (p) { return p.dataset.key === wanted; })[0] || pills[0];
    clearInterval(t);
    pill.click();
  }, 60);
})();
