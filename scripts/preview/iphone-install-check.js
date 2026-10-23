// Checks Get the iPhone app in Settings › Devices (DevicesSection.tsx): on the card for linking a
// phone, it opens the quick panel on a chat of its own with the prompt that installs the app with
// you (devices.ts `IPHONE_PROMPT`). On Linux, which can't build it, there's no button.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/iphone-install-check.js
//   PREVIEW_PLATFORM=linux PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/iphone-install-check.js
(function () {
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var phoneCard = function () {
    return all('.ca-settings .ca-ucard').filter(function (c) { var n = c.querySelector('.ca-device-name'); return n && /^Your phone/.test(n.textContent); })[0];
  };
  var getApp = function () {
    var card = phoneCard();
    return card && Array.prototype.filter.call(card.querySelectorAll('button'), function (b) { return b.textContent.trim() === 'Get the iPhone app…'; })[0];
  };
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);

  say('iphone install check: waiting for Settings');
  until(function () { return !!q('.ca-side-config'); }, 20000, 'no Settings button')
    .then(function () {
      q('.ca-side-config').click();
      return until(function () { return !!nav('Devices'); }, 3000, 'no Devices in Settings');
    })
    .then(function () {
      nav('Devices').click();
      return until(function () { return !!phoneCard(); }, 5000, 'no card for linking a phone (run with PREVIEW_DEVICES)');
    })
    .then(function () {
      if (linux) {
        check('no Get the iPhone app on Linux, which has no Xcode', !getApp(), phoneCard().textContent);
        return done();
      }
      var button = getApp();
      check('the phone card offers Get the iPhone app', !!button, phoneCard().textContent);
      check('and says why it takes an agent', /isn’t on the App Store yet/.test(phoneCard().textContent), phoneCard().textContent);
      button.click();
      return until(function () { return calls('show_quick').length; }, 3000, 'the quick panel asked for').then(function () {
        var ctx = sent('show_quick')[0].context;
        check('opens the quick panel on a chat of its own', ctx.chat === true && !ctx.laneId, JSON.stringify(ctx));
        check('with the prompt that installs the app from its repo', /github\.com\/codealong\/codealong-ios/.test(ctx.prompt) && /scripts\/install-device\.sh/.test(ctx.prompt), ctx.prompt);
        check('and links it here', /codealong devices code/.test(ctx.prompt), ctx.prompt);
        done();
      });
    })
    .catch(stopped);
})();
