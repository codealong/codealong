// Opens Set up Codealong (ui/SetupDialog.tsx) at its session emblems step with Emoji picked, which
// asks for Jev's key (the stub's keychain has none), for a screenshot:
//   PREVIEW_SETUP=scripts/preview/setup-sample.json PREVIEW_INIT=scripts/preview/setup-emblems-init.js PREVIEW_NAME=setup-emblems scripts/preview.sh
// window.__SETUP_EMBLEMS = 'drawn' (or 'off') picks that instead, from a PREVIEW_INIT of your own.
(function () {
  var dialog = function () { return document.querySelector('.ca-setup'); };
  var heading = function () { var h = dialog() && dialog().querySelector('.ca-dialog-head h2'); return h ? h.textContent.trim() : ''; };
  var button = function (label) { return Array.prototype.filter.call(dialog().querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var label = { off: 'None', emoji: 'Emoji', drawn: 'Drawn' }[window.__SETUP_EMBLEMS || 'emoji'];
  var tries = 0;
  var step = setInterval(function () {
    tries += 1;
    if (tries > 2000) return clearInterval(step);
    if (!dialog()) {
      if (document.querySelector('.ca-usagebar-settings')) window.__emit('setup', null);
      return;
    }
    if (heading() === 'Set up Codealong') return;
    if (heading() === 'Your sessions') {
      var pick = Array.prototype.filter.call(dialog().querySelectorAll('.ca-segctl button'), function (b) { return b.textContent.trim() === label; })[0];
      if (pick) pick.click();
      return clearInterval(step);
    }
    var next = button('Next');
    if (next) next.click();
  }, 10);
})();
