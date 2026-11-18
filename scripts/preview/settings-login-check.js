// Checks Settings › General's Open at login (os/login): off as a new install has it, ticking
// it asks the app to turn it on and shows it on once the system says so, and a change made in
// System Settings meanwhile (turned off, or waiting to be allowed) shows once the window is back.
//   scripts/preview.sh sessions '' scripts/preview/settings-login-check.js
(function () {
  var field = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === 'Open at login'; })[0];
  };
  var box = function () { return field().querySelector('input[type=checkbox]'); };

  until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')
    .then(function (gear) {
      gear.click();
      return until(field, 20000, 'Open at login in General');
    })
    .then(function (f) {
      check('off until you turn it on', !box().checked, f.textContent);
      check('says where the system lists it', /Login Items/.test(f.textContent), f.textContent);
      box().click();
      return until(function () { return calls('set_open_at_login').length; }, 5000, 'the ask to turn it on');
    })
    .then(function () {
      check('asks for it on', sent('set_open_at_login')[0].on === true, JSON.stringify(sent('set_open_at_login')));
      return until(function () { return box().checked; }, 5000, 'the box ticked');
    })
    .then(function () {
      check('shows it on', box().checked, field().textContent);
      // Turned off in System Settings meanwhile: coming back to the window shows it.
      window.__atLogin = 'off';
      window.dispatchEvent(new Event('focus'));
      return until(function () { return !box().checked; }, 5000, 'the box unticked after focus');
    })
    .then(function () {
      check('reads the system again on focus', !box().checked, field().textContent);
      window.__atLogin = 'approve';
      window.dispatchEvent(new Event('focus'));
      return until(function () { return /waits for you to allow/.test(field().textContent); }, 5000, 'the ask to allow it');
    })
    .then(function () {
      check('says macOS waits to allow it', box().checked && /System Settings › General › Login Items/.test(field().textContent), field().textContent);
    })
    .then(done)
    .catch(stopped);
})();
