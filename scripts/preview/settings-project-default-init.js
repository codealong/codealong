// Opens Settings › Projects on a made-up project's Edit form with *Auto starts on* set to a group
// of its own, for a screenshot of the field (config.rs `ProjectCfg::default`), over made-up
// groups and projects whatever this Mac has. project-default-check.js checks what it does.
//   PREVIEW_INIT=scripts/preview/settings-project-default-init.js PREVIEW_NAME=settings-project-default scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call(document.querySelectorAll('button'), function (b) { return b.textContent.trim() === label && (!within || within.contains(b)); })[0]; };
  var card = function (name) { return Array.prototype.filter.call(document.querySelectorAll('.ca-ucard'), function (c) { var n = c.querySelector('.ca-ucard-name'); return n && n.textContent.trim() === name; })[0]; };
  var field = function (label) { return Array.prototype.filter.call(document.querySelectorAll('.ca-settings-pane .ca-field'), function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === label; })[0]; };
  fetch('./config.json')
    .then(function (r) { return r.json(); })
    .then(function (view) {
      var cfg = view.config;
      cfg.routing = Object.assign({}, cfg.routing, { auto: 'personal', groups: [{ name: 'personal', strategy: 'usage', members: [] }, { name: 'work', strategy: 'usage', members: [] }] });
      cfg.projects = [
        { name: 'Northwind', repos: ['github.com/northwind/*'], folders: [], default: 'work' },
        { name: 'Atlas', repos: ['github.com/atlas/*'], folders: [] },
      ];
      window.__keepConfig = true;
      window.__keptConfig = cfg;
      var step = 0;
      var go = setInterval(function () {
        if (step === 0 && document.querySelector('.ca-side-config')) {
          document.querySelector('.ca-side-config').click();
          step = 1;
        } else if (step === 1 && button('Projects')) {
          button('Projects').click();
          step = 2;
        } else if (step === 2 && card('Northwind')) {
          button('Edit', card('Northwind')).click();
          step = 3;
        } else if (step === 3 && field('Auto starts on')) {
          field('Auto starts on').scrollIntoView({ block: 'center' });
          clearInterval(go);
        }
      }, 100);
    });
})();
