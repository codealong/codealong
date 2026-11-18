// Opens Settings › Accounts on an account whose command line isn't on this machine (made up here,
// as accounts-cli-check.js has it), for a screenshot (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/settings-accounts-cli-init.js PREVIEW_NAME=settings-accounts-cli scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-accounts-cli-init.js PREVIEW_NAME=settings-accounts-cli scripts/preview.sh sessions
(function () {
  var info = function (name, harness, home, over) {
    return Object.assign({ name: name, harness: harness, home: home, command: null, start: null, acp: null, monthly: null, per_token: false, disabled: false, folderExists: true, signedIn: true, email: null, signInNote: null }, over);
  };
  var accounts = [
    info('claude', 'claude', '~/.claude', { command: 'claude', email: 'ana@example.com' }),
    info('cursor', 'cursor', '~/.cursor', { command: 'cursor-agent', email: 'ana@example.com', missingCli: 'cursor-agent' }),
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'get_accounts') return Promise.resolve(JSON.parse(JSON.stringify(accounts)));
    if (cmd === 'found_accounts') return Promise.resolve([]);
    return real(cmd, args, options);
  };
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var nav = document.querySelector('.ca-settings-nav');
    var card = document.getElementById('ca-acct-cursor');
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && nav && button('Accounts', nav)) {
      button('Accounts', nav).click();
      step = 2;
    } else if (step === 2 && card) {
      card.scrollIntoView({ block: 'start' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
