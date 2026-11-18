// Opens Settings › Accounts' form for a new Claude Code account, which offers the settings of an
// account there to start from, for a screenshot (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-account-copy-init.js PREVIEW_NAME=settings-account-copy scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-account-copy-init.js PREVIEW_NAME=settings-account-copy scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var nav = document.querySelector('.ca-settings-nav');
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && nav && button('Accounts', nav)) {
      button('Accounts', nav).click();
      step = 2;
    } else if (step === 2 && button('Add Claude Code account')) {
      button('Add Claude Code account').click();
      step = 3;
    } else if (step === 3 && document.querySelector('.ca-acct-form')) {
      document.querySelector('.ca-acct-form').scrollIntoView({ block: 'start' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
