// Checks keyboard shortcuts show as Settings says (src/keysShown.ts, `[appearance] show_keys`): off,
// as a new config.toml has it, the session strip's keys are hidden, holding ⌘ alone a moment shows
// them, ⇧ pressed during the hold doesn't call it off, letting go hides them, and a chord pressed
// at once shows nothing; on, they show all along.
//   scripts/preview.sh sessions '' scripts/preview/keys-shown-check.js
//   PREVIEW_KEYS=1 scripts/preview.sh sessions '' scripts/preview/keys-shown-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var always = !!window.__PREVIEW_KEYS__;
  var key = function () { return q('.ca-reply-queue .ca-hotkey kbd'); };
  var shows = function (el) { return !!el && getComputedStyle(el).display !== 'none'; };
  // A key that does nothing in the app, to stand for the rest of a chord.
  var press = function (type, k) { window.dispatchEvent(new KeyboardEvent(type, { key: k, bubbles: true })); };
  say('keys shown check: waiting for a session');
  until(function () { return q('.ca-lane[id^="row-"]'); }, 20000, 'no session rows')
    .then(function () {
      q('.ca-lane[id^="row-"]').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(key, 5000, 'no strip with keys over the session');
    })
    .then(function () {
      if (always) {
        check('with the setting on, the strip’s keys show', shows(key()) && document.documentElement.dataset.keys === 'always', document.documentElement.dataset.keys);
        return 'done';
      }
      check('off, as a new config.toml has it, the strip’s keys are hidden', !shows(key()) && !document.documentElement.dataset.keys);
      press('keydown', 'Meta');
      return wait(200);
    })
    .then(function (end) {
      if (end) return end;
      check('not yet while ⌘ has been held only a moment', !shows(key()));
      return wait(400);
    })
    .then(function (end) {
      if (end) return end;
      check('held a moment, ⌘ shows them', shows(key()) && document.documentElement.dataset.keys === 'held', document.documentElement.dataset.keys);
      press('keyup', 'Meta');
      return wait(20);
    })
    .then(function (end) {
      if (end) return end;
      check('and letting go hides them', !shows(key()));
      // ⇧ pressed during the hold, for a chord worth seeing, waits on.
      press('keydown', 'Meta');
      return wait(200);
    })
    .then(function (end) {
      if (end) return end;
      press('keydown', 'Shift');
      return wait(400);
    })
    .then(function (end) {
      if (end) return end;
      check('⇧ pressed during the hold still shows them', shows(key()));
      press('keyup', 'Shift');
      press('keyup', 'Meta');
      return wait(20);
    })
    .then(function (end) {
      if (end) return end;
      press('keydown', 'Meta');
      press('keydown', 'F19');
      return wait(700);
    })
    .then(function (end) {
      if (!end) {
        check('a chord pressed at once shows nothing', !shows(key()));
        press('keyup', 'F19');
        press('keyup', 'Meta');
      }
      done();
    })
    .catch(stopped);
})();
