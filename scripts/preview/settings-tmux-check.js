// Checks Settings › General's Run sessions in tmux (`[apps] tmux`): only while the screen is
// locked as a new install has it, each choice saying what it comes to in the terminal's name,
// and a pick saved to config.toml. With a terminal that holds sessions itself (Rex) as
// config.toml's, the choice says tmux isn't used instead. On Linux, where a terminal is tmux
// already, it isn't there.
//   scripts/preview.sh sessions '' scripts/preview/settings-tmux-check.js
//   PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/settings-tmux-check.js
(function () {
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  var field = function () {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === 'Run sessions in tmux'; })[0];
  };
  var choice = function (label) {
    return Array.prototype.filter.call(field().querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0];
  };
  var picked = function () {
    var on = field().querySelector('button[aria-pressed=true], button[aria-checked=true], button.is-on, button[data-on]');
    return on ? on.textContent.trim() : '';
  };

  // What the stub answers get_config with: which terminal config.toml names, and whether it
  // holds sessions itself (config.rs `holds_sessions`).
  var holds = false;
  fetch('./config.json')
    .then(function (r) { return r.json(); })
    .then(function (view) {
      holds = (view.holdsSessions || []).indexOf(view.config.apps.terminal) >= 0;
      return until(function () { return q('.ca-side-config'); }, 20000, 'the settings button');
    })
    .then(function (gear) {
      gear.click();
      return until(function () { return all('.ca-settings-pane .ca-field > span:not(.ca-checkbox)').filter(function (s) { return s.textContent === 'Terminal'; })[0]; }, 20000, 'the terminal settings in General');
    })
    .then(function () {
      if (linux) {
        check('not offered on Linux, whose terminals are tmux sessions', !field());
        return done();
      }
      var f = field();
      check('offered beside the terminal settings', !!f);
      check('only while locked to begin with', picked() === 'While locked', picked() + ' | ' + f.textContent);
      if (holds) check('says the terminal holds sessions itself, so tmux isn’t used', /starts them while the screen is locked too/.test(f.textContent) && /tmux isn’t used/.test(f.textContent), f.textContent);
      else check('says what that comes to, in the terminal’s name', /While the screen is locked/.test(f.textContent) && /opens in \S+ once you unlock/.test(f.textContent), f.textContent);
      choice('Always').click();
      return until(function () { return calls('save_config').length; }, 5000, 'the pick saved');
    })
    .then(function () {
      if (linux) return;
      var saved = sent('save_config')[0].config.apps;
      check('saves always to [apps] tmux', saved.tmux === 'always', JSON.stringify(saved));
      done();
    })
    .catch(stopped);
})();
