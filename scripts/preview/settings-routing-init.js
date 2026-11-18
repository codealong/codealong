// Opens Settings › Routing on a new fallback group with three accounts added from the agent menu
// (the first account on Auto's model, the second on a model of its own, the third on Auto's at
// high effort), for a screenshot (preview.sh's sessions view). PREVIEW_MENU=1 leaves the first
// one's menu open, its effort row under it.
//   PREVIEW_INIT=scripts/preview/settings-routing-init.js PREVIEW_NAME=settings-routing PREVIEW_SIZE=1440,1400 scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-routing-init.js PREVIEW_NAME=settings-routing PREVIEW_SIZE=1440,1400 scripts/preview.sh sessions
(function () {
  var menuOpen = /PREVIEW_MENU=1/.test(location.search) || window.__PREVIEW_MENU__;
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var adder = function (form) { return Array.prototype.filter.call(form.querySelectorAll('.ca-modelmenu-chip'), function (b) { return /Add account/.test(b.textContent); })[0]; };
  var accountLines = function () {
    var out = [];
    Array.prototype.forEach.call(document.querySelectorAll('.ca-picker-section'), function (s) {
      var opts = Array.prototype.filter.call(s.querySelectorAll('.ca-picker-opt'), function (o) { return !o.disabled; });
      if (s.querySelector('.ca-quick-acct-name') && opts.length) out.push(opts);
    });
    return out;
  };
  var rows = function (form) { return form.querySelectorAll('.ca-routing-member-start'); };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var form = document.querySelector('.ca-acct-form');
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
    } else if (step >= 4 && step < 7 && form && rows(form).length === step - 4) {
      if (!document.querySelector('.ca-picker')) {
        adder(form).click();
        return;
      }
      var lines = accountLines();
      var acct = lines[(step - 4) % lines.length];
      // The second on a model of its own; the others on Auto's.
      (step === 5 && acct[1] ? acct[1] : acct[0]).click();
      step += 1;
    } else if (step === 7 && form && rows(form).length === 3) {
      if (!document.querySelector('.ca-picker')) {
        rows(form)[2].querySelector('.ca-modelmenu-chip').click();
        return;
      }
      var high = Array.prototype.filter.call(document.querySelectorAll('.ca-picker-foot button'), function (b) { return b.textContent.trim() === 'High'; })[0];
      if (high) high.click();
      rows(form)[2].querySelector('.ca-modelmenu-chip').click();
      step = 8;
    } else if (step === 8) {
      form.querySelectorAll('.ca-field')[1].scrollIntoView({ block: 'start' });
      if (menuOpen) rows(form)[0].querySelector('.ca-modelmenu-chip').click();
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
