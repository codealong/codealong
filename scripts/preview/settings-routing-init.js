// Opens Settings › Routing on a new fallback group with three accounts checked, the second moved
// first, for a screenshot (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-routing-init.js PREVIEW_NAME=settings-routing PREVIEW_SIZE=1440,1400 scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-routing-init.js PREVIEW_NAME=settings-routing PREVIEW_SIZE=1440,1400 scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var form = document.querySelector('.ca-acct-form');
    var rows = form ? form.querySelectorAll('.ca-routing-member') : [];
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && button('Routing')) {
      button('Routing').click();
      step = 2;
    } else if (step === 2 && button('Add routing group')) {
      button('Add routing group').click();
      step = 3;
    } else if (step === 3 && form) {
      var name = form.querySelector('input');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(name, 'overflow');
      name.dispatchEvent(new Event('input', { bubbles: true }));
      button('Fallback', form).click();
      step = 4;
    } else if (step >= 4 && step < 7 && rows.length > step - 4) {
      rows[step - 4].querySelector('input[type=checkbox]').click();
      step += 1;
    } else if (step === 7) {
      var later = rows[0] && rows[0].querySelector('button[aria-label$="later"]');
      if (later) later.click();
      step = 8;
    } else if (step === 8) {
      form.querySelectorAll('.ca-field')[1].scrollIntoView({ block: 'start' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
