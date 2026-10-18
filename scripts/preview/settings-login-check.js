// Checks Settings › General's Open at login (os/login): off as a new install has it, and ticking
// it asks the app to turn it on and shows it on once the system says so.
//   scripts/preview.sh sessions '' scripts/preview/settings-login-check.js
(function () {
  var field = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === 'Open at login'; })[0];
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
    })
    .then(done)
    .catch(stopped);
})();
