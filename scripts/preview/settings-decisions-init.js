// Opens Settings › Decisions over made-up decision models, for a screenshot: Jev the default with
// TypeSafe's key saved, OpenAI's model chosen by a project and falling back to Jev, and no key for
// OpenAI yet (config.rs `DecisionsCfg`, decisions.ts). In a page 1000 or more high (a window of 1100) it scrolls on
// to the keys, the usage and the switches. settings-decisions-check.js checks what it does;
// settings-decisions-project-init.js shows a project's Decision model.
//   PREVIEW_INIT=scripts/preview/settings-decisions-init.js PREVIEW_NAME=settings-decisions scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-decisions-init.js PREVIEW_NAME=settings-decisions scripts/preview.sh sessions
//   PREVIEW_SIZE=1440,1100 PREVIEW_INIT=scripts/preview/settings-decisions-init.js PREVIEW_NAME=settings-decisions-keys scripts/preview.sh sessions
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'decision_keys') return Promise.resolve({ typesafe: true, openai: false, gemini: false });
    return real(cmd, args, options);
  };
  var button = function (label) { return Array.prototype.filter.call(document.querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (label) { return Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field'), function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === label; })[0]; };
  // The board's own theme wins over the scheme Chrome is given: light unless the run asks for dark.
  var theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  fetch('./config.json')
    .then(function (r) { return r.json(); })
    .then(function (view) {
      var cfg = view.config;
      cfg.appearance = Object.assign({}, cfg.appearance, { theme: theme });
      cfg.decisions = { default: 'jev', quick_panel: true, sessions: true, todos: false, pull_requests: false, prompts: false, models: [{ name: 'luna', provider: 'openai', model: 'gpt-6-luna', fallback: 'jev' }] };
      cfg.projects = [
        { name: 'Atlas', repos: ['github.com/atlas/*'], folders: [], decisions: 'luna' },
        { name: 'Kestrel', repos: ['github.com/kestrel/*'], folders: [], decisions: 'none' },
        { name: 'Northwind', repos: ['github.com/northwind/*'], folders: [] },
      ];
      window.__keepConfig = true;
      window.__keptConfig = cfg;
      var step = 0;
      var go = setInterval(function () {
        if (step === 0 && document.querySelector('.ca-side-config')) {
          window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) {
            b.theme = theme;
            window.__emit('board', b);
          });
          document.querySelector('.ca-side-config').click();
          step = 1;
        } else if (step === 1 && button('Decisions')) {
          button('Decisions').click();
          step = 2;
        } else if (step >= 2 && step < 12 && field('Default model')) {
          // Again for a while, as the pane fills in (the usage reads after it shows).
          step += 1;
          if (window.innerHeight < 1000) return clearInterval(go);
          var f = field('Default model');
          var box = f.parentElement;
          while (box && box.scrollHeight <= box.clientHeight + 1) box = box.parentElement;
          if (box) box.scrollTop = f.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop - 8;
        } else if (step >= 12) {
          clearInterval(go);
        }
      }, 100);
    });
})();
