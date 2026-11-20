// Opens Settings › Projects on a made-up project's Edit form at its Decision model (config.rs
// `ProjectCfg::decisions`), over made-up decision models and projects, for a screenshot.
//   PREVIEW_INIT=scripts/preview/settings-decisions-project-init.js PREVIEW_NAME=settings-decisions-project scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/settings-decisions-project-init.js PREVIEW_NAME=settings-decisions-project-dark scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call(document.querySelectorAll('button'), function (b) { return b.textContent.trim() === label && (!within || within.contains(b)); })[0]; };
  var card = function (name) { return Array.prototype.filter.call(document.querySelectorAll('.ca-ucard'), function (c) { var n = c.querySelector('.ca-ucard-name'); return n && n.textContent.trim() === name; })[0]; };
  var field = function (label) { return Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field'), function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === label; })[0]; };
  fetch('./config.json')
    .then(function (r) { return r.json(); })
    .then(function (view) {
      var cfg = view.config;
      cfg.decisions = { default: 'jev', quick_panel: true, sessions: true, todos: false, pull_requests: false, prompts: false, models: [{ name: 'luna', provider: 'openai', model: 'gpt-6-luna', fallback: 'jev' }] };
      cfg.projects = [
        { name: 'Atlas', repos: ['github.com/atlas/*'], folders: [], decisions: 'luna' },
        { name: 'Northwind', repos: ['github.com/northwind/*'], folders: [] },
      ];
      window.__keepConfig = true;
      window.__keptConfig = cfg;
      // The board's own theme wins over the scheme Chrome is given: light unless the run asks for dark.
      var dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      cfg.appearance = Object.assign({}, cfg.appearance, { theme: dark ? 'dark' : 'light' });
      window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) {
        b.theme = dark ? 'dark' : 'light';
        window.__emit('board', b);
      });
      var step = 0;
      var go = setInterval(function () {
        if (step === 0 && document.querySelector('.ca-side-config')) {
          document.querySelector('.ca-side-config').click();
          step = 1;
        } else if (step === 1 && button('Projects')) {
          button('Projects').click();
          step = 2;
        } else if (step === 2 && card('Atlas')) {
          button('Edit', card('Atlas')).click();
          step = 3;
        } else if (step === 3 && field('Decision model')) {
          field('Decision model').scrollIntoView({ block: 'center' });
          clearInterval(go);
        }
      }, 100);
    });
})();
