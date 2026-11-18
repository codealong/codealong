// Checks Settings › General's Keep <terminal> awake (os/naps.rs): under the Terminal picker,
// named for the terminal picked, off as macOS has an app by default, ticking it asks the app to
// keep that terminal awake and says the terminal takes it up as it opens, and a change made with
// `defaults` meanwhile shows once the window is back. Linux naps nothing, so it isn't there.
//   scripts/preview.sh sessions '' scripts/preview/settings-awake-check.js
//   PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/settings-awake-check.js
(function () {
  if (window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent)) {
    until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')
      .then(function (gear) {
        gear.click();
        return until(function () { return all('.ca-settings-pane .ca-field > span').filter(function (s) { return s.textContent === 'Terminal'; })[0]; }, 20000, 'the Terminal picker');
      })
      .then(function () {
        check('no switch to keep the terminal awake', !all('.ca-settings-pane .ca-field > span').some(function (s) { return /^Keep .+ awake/.test(s.textContent); }));
        check('nothing asked of the system', calls('terminal_awake').length === 0, JSON.stringify(sent('terminal_awake')));
      })
      .then(done)
      .catch(stopped);
    return;
  }

  var field = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && /^Keep .+ awake behind other windows$/.test(s.textContent); })[0];
  };
  var box = function () { return field().querySelector('input[type=checkbox]'); };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var first, other, release;
  var terminalField = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === 'Terminal'; })[0];
  };

  until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')
    .then(function (gear) {
      gear.click();
      return until(field, 20000, 'Keep … awake in General');
    })
    .then(function (f) {
      var picked = terminalField().querySelector('select');
      var name = picked.options[picked.selectedIndex].textContent;
      check('named for the terminal picked', f.querySelector(':scope > span:not(.ca-checkbox)').textContent === 'Keep ' + name + ' awake behind other windows', f.textContent);
      check('right under the Terminal picker', terminalField().nextElementSibling === f, f.previousElementSibling && f.previousElementSibling.textContent);
      check('asked of the system for that terminal', sent('terminal_awake').some(function (a) { return a.terminal === picked.value; }), JSON.stringify(sent('terminal_awake')));
      check('off until you turn it on', !box().checked, f.textContent);
      check('says why', /slows .+ down while other apps’ windows cover it/.test(f.textContent), f.textContent);
      box().click();
      return until(function () { return calls('set_terminal_awake').length; }, 5000, 'the ask to keep it awake');
    })
    .then(function () {
      var asked = sent('set_terminal_awake')[0];
      check('asks for the terminal picked to stay awake', asked.awake === true && asked.terminal === terminalField().querySelector('select').value, JSON.stringify(asked));
      return until(function () { return box().checked; }, 5000, 'the box ticked');
    })
    .then(function () {
      check('shows it on, and says when it takes', /(next time .+ opens|Quit and reopen)/.test(field().textContent) && !/slows/.test(field().textContent), field().textContent);
      // Changed with `defaults` meanwhile: coming back to the window shows it.
      window.__awake = false;
      window.dispatchEvent(new Event('focus'));
      return until(function () { return !box().checked; }, 5000, 'the box unticked after focus');
    })
    .then(function () {
      check('reads the system again on focus', !box().checked, field().textContent);
      // A change still on its way for one terminal while you pick another: its answer stays that
      // terminal's, and the other's switch shows its own setting.
      var picker = terminalField().querySelector('select');
      first = picker.value;
      other = Array.prototype.filter.call(picker.options, function (o) { return o.value !== first && o.value !== 'tmux'; })[0].value;
      window.__keepConfig = true;
      window.__awake = {};
      window.__awake[first] = false;
      window.__awake[other] = false;
      window.__awakeHold = new Promise(function (go) { release = go; });
      box().click();
      return until(function () { return calls('set_terminal_awake').length === 2; }, 5000, 'the second ask, held');
    })
    .then(function () {
      pick(terminalField().querySelector('select'), other);
      return until(function () { return field() && sent('terminal_awake').some(function (a) { return a.terminal === other; }) && field().querySelector(':scope > span:not(.ca-checkbox)').textContent.indexOf('Keep ' + first) < 0; }, 5000, 'the other terminal’s switch, read');
    })
    .then(function () {
      release();
      return wait(200);
    })
    .then(function () {
      check('the held change went to the terminal it was asked for', window.__awake[first] === true && window.__awake[other] === false, JSON.stringify(window.__awake));
      check('the other terminal’s switch shows its own setting', !box().checked && /slows/.test(field().textContent), field().textContent);
    })
    .then(done)
    .catch(stopped);
})();
