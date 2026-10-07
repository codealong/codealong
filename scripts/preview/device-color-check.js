// Checks a linked device's color in Settings › Devices (DevicesSection.tsx): a color picked from
// its dot's menu asks the engine for it, and one the engine refuses (it couldn't save it) says why
// in the section, as Link and Unlink do; the next that goes through clears it.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh worktrees '' scripts/preview/device-color-check.js
(function () {
  var refuse = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'set_device_color' || !refuse) return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return Promise.reject('Couldn’t save the devices’ colors: the disk is full.');
  };

  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var dot = function () { return q('.ca-settings .ca-devcolor'); };
  var said = function () { var p = q('.ca-settings-pane .ca-form-error'); return p ? p.textContent : ''; };
  var pick = function (label) {
    dot().click();
    return wait(100).then(function () {
      var item = window.__menu().filter(function (i) { return i[0].replace(/ ✓$/, '') === label; })[0];
      if (!item) throw new Error('no ' + label + ' in ' + window.__menu().map(function (i) { return i[0]; }).join(', '));
      item[1]();
      return wait(200);
    });
  };

  say('device color check: waiting for Settings');
  until(function () { return !!q('.ca-side-config'); }, 20000, 'no Settings button')
    .then(function () {
      q('.ca-side-config').click();
      return until(function () { return !!nav('Devices'); }, 3000, 'no Devices in Settings');
    })
    .then(function () {
      nav('Devices').click();
      return until(function () { return !!dot(); }, 3000, 'no linked device with a color (run with PREVIEW_DEVICES)');
    })
    .then(function () { return pick('Teal'); })
    .then(function () {
      var asked = sent('set_device_color');
      check('a color picked asks the engine for it', asked.length === 1 && asked[0].color === 'teal', JSON.stringify(asked));
      check('and says nothing when it goes', said() === '', said());
      refuse = true;
      return pick('Purple');
    })
    .then(function () {
      check('one the engine refuses says why', /disk is full/.test(said()), said() || 'nothing');
      refuse = false;
      return pick('Green');
    })
    .then(function () {
      check('the next that goes clears it', said() === '' && sent('set_device_color').length === 3, said());
    })
    .then(done, stopped);
})();
