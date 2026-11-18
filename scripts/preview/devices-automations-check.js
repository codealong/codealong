// Checks where this machine stands for the automations your machines share, in Settings › Devices
// (DevicesSection.tsx, `[devices] automations`): first choice to begin with, saying what that comes
// to; a pick saved to config.toml; and each linked machine's own place on its card.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/devices-automations-check.js
(function () {
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === 'For automations my machines share'; })[0];
  };
  var choice = function (label) {
    return Array.prototype.filter.call(field().querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0];
  };
  var picked = function () {
    var on = field().querySelector('button[aria-checked=true]');
    return on ? on.textContent.trim() : '';
  };
  var card = function (name) {
    return all('.ca-settings .ca-ucard').filter(function (c) { var n = c.querySelector('.ca-device-name'); return n && n.textContent === name; })[0];
  };

  say('devices automations check: waiting for Settings');
  until(function () { return !!q('.ca-side-config'); }, 20000, 'no Settings button')
    .then(function () {
      q('.ca-side-config').click();
      return until(function () { return !!nav('Devices'); }, 3000, 'no Devices in Settings');
    })
    .then(function () {
      nav('Devices').click();
      return until(function () { return !!field() && !!card('Mac mini') && !!card('Studio'); }, 5000, 'no place in line for shared automations, or no linked machines (run with PREVIEW_DEVICES)');
    })
    .then(function () {
      var f = field();
      check('a first choice to begin with', picked() === 'First choice', picked());
      check('says what it shares and what a first choice does', /reviewing a pull request that asks for your review/.test(f.textContent) && /runs them whenever it can/.test(f.textContent), f.textContent);
      check('each linked machine says its own place', /First choice/.test(card('Mac mini').textContent) && /Backup/.test(card('Studio').textContent), card('Mac mini').textContent + ' | ' + card('Studio').textContent);
      choice('Backup').click();
      return until(function () { return calls('save_config').length; }, 5000, 'the pick saved');
    })
    .then(function () {
      var saved = sent('save_config')[0].config.devices;
      check('saves backup to [devices] automations, the rest as it was', saved.automations === 'backup' && saved.enabled === true, JSON.stringify(saved));
      return until(function () { return /only while no first choice can/.test(field().textContent); }, 3000, 'what a backup does');
    })
    .then(function () {
      check('and says what a backup does', picked() === 'Backup', picked());
      done();
    })
    .catch(stopped);
})();
