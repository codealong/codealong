// Checks that Settings › General offers to put back the shortcuts this OS's engine defaults to
// (`ConfigView.defaultShortcuts`), not macOS's: with Linux's defaults and the quick panel's moved
// to F18, Use puts back Ctrl+Alt+/, and a shortcut left at its default offers nothing.
//   scripts/preview.sh sessions '' scripts/preview/settings-shortcut-check.js
(function () {
  var linux = { quick_panel: 'Ctrl+Alt+/', chat: "Ctrl+Alt+'", waiting: 'Ctrl+Alt+.', improve: 'Ctrl+Alt+,', main_window: 'Ctrl+Alt+;' };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      view.defaultShortcuts = Object.assign({}, linux);
      view.config.shortcuts = Object.assign({}, linux, { quick_panel: 'F18' });
      return view;
    });
  };
  var field = function (title) { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === title; })[0]; };
  var use = function (f) { return all('button', f).filter(function (b) { return /^Use /.test(b.textContent.trim()); })[0]; };

  until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')
    .then(function (gear) {
      gear.click();
      return until(function () { return field('Quick panel'); }, 20000, 'the quick panel shortcut in General');
    })
    .then(function (quick) {
      var button = use(quick);
      check('a moved shortcut offers its default', !!button, quick.textContent);
      check('the chat shortcut, at its default, offers nothing', !use(field('Chat')), field('Chat').textContent);
      if (!button) return;
      button.click();
      return until(function () { return calls('save_config').length; }, 5000, 'a save').then(function () {
        var saved = sent('save_config').slice(-1)[0].config.shortcuts;
        check('Use puts back this OS’s default', saved.quick_panel === 'Ctrl+Alt+/', JSON.stringify(saved));
      });
    })
    .then(done)
    .catch(stopped);
})();
