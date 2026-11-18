// Opens Settings › Devices on the linked machines' cards (PREVIEW_DEVICES), for a screenshot
// (preview.sh's sessions view).
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_INIT=scripts/preview/settings-devices-init.js PREVIEW_NAME=settings-devices scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_INIT=scripts/preview/settings-devices-init.js PREVIEW_NAME=settings-devices scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var nav = document.querySelector('.ca-settings-nav');
    var card = document.querySelector('.ca-settings .ca-device-name');
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && nav && button('Devices', nav)) {
      button('Devices', nav).click();
      step = 2;
    } else if (step === 2 && card) {
      card.closest('.ca-ucard').scrollIntoView({ block: 'start' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
