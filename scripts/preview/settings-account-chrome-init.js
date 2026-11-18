// Opens Settings › Accounts editing a made-up Claude Code account, its Chrome profile set to Work,
// for a screenshot of the field (preview.sh's sessions view); the card above it says where its
// pages open.
//   PREVIEW_INIT=scripts/preview/settings-account-chrome-init.js PREVIEW_NAME=settings-account-chrome scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-account-chrome-init.js PREVIEW_NAME=settings-account-chrome scripts/preview.sh sessions
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: name, start: null, acp: null, monthly: null, per_token: false, disabled: false, makesPages: harness === 'claude', folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { email: 'ana@example.com', chrome_profile: 'Personal' }),
    info('claude2', 'claude', '~/.claude2', { email: 'ana@work.example', chrome_profile: 'Work' }),
  ];
  var profiles = [
    { id: 'Personal', name: 'Personal', dir: 'Default', email: 'ana@example.com' },
    { id: 'Work', name: 'Work', dir: 'Profile 3', email: 'ana@work.example' },
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    if (cmd === 'chrome_profiles') return Promise.resolve(JSON.parse(JSON.stringify(profiles)));
    return real(cmd, args, options);
  };
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var nav = document.querySelector('.ca-settings-nav');
    var card = document.getElementById('ca-acct-claude2');
    var form = document.querySelector('.ca-acct-form');
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && nav && button('Accounts', nav)) {
      button('Accounts', nav).click();
      step = 2;
    } else if (step === 2 && card && button('Edit', card)) {
      button('Edit', card).click();
      step = 3;
    } else if (step === 3 && form && form.querySelector('select')) {
      var fields = Array.prototype.slice.call(form.querySelectorAll('.ca-field'));
      var chrome = fields.filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent.trim() === 'Chrome profile'; })[0];
      (chrome || form).scrollIntoView({ block: 'end' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
