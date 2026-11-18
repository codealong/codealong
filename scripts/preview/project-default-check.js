// Checks a project's own routing group for Auto (config.rs `ProjectCfg::default`): Settings ›
// Projects' Edit form offers Routing's rule and each group, saving one writes it as the project's
// `default` and its card says so, and removing that group in Settings › Routing takes it off the
// project too, back to Routing's. Made-up projects and groups, over whatever this Mac has.
//   scripts/preview.sh sessions '' scripts/preview/project-default-check.js
(function () {
  var button = function (label, within) { return all('button').filter(function (b) { return b.textContent.trim() === label && (!within || within.contains(b)); })[0]; };
  var card = function (name) { return all('.ca-ucard').filter(function (c) { var n = c.querySelector('.ca-ucard-name'); return n && n.textContent.trim() === name; })[0]; };
  var field = function (label) { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === label; })[0]; };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var saved = function () { var s = sent('save_config'); return s.length ? s[s.length - 1].config : null; };

  fetch('./config.json')
    .then(function (r) { return r.json(); })
    .then(function (view) {
      var cfg = view.config;
      cfg.routing = Object.assign({}, cfg.routing, {
        auto: 'personal',
        groups: [
          { name: 'personal', strategy: 'usage', members: [] },
          { name: 'work', strategy: 'usage', members: [] },
        ],
      });
      cfg.projects = [
        { name: 'Northwind', repos: ['github.com/northwind/*'], folders: [] },
        { name: 'Atlas', repos: ['github.com/atlas/*'], folders: [] },
      ];
      window.__keepConfig = true;
      window.__keptConfig = cfg;
      return until(function () { return q('.ca-side-config'); }, 20000, 'the settings button');
    })
    .then(function (gear) {
      gear.click();
      return until(function () { return button('Projects'); }, 20000, 'Settings › Projects');
    })
    .then(function (projects) {
      projects.click();
      return until(function () { return card('Northwind'); }, 20000, 'Northwind’s card');
    })
    .then(function (c) {
      check('no Auto line on a project that follows Routing', !/Group:/.test(c.textContent), c.textContent);
      button('Edit', c).click();
      return until(function () { return field('Auto starts on'); }, 5000, 'the Auto field in its form');
    })
    .then(function (f) {
      var select = f.querySelector('select');
      var options = Array.prototype.map.call(select.options, function (o) { return o.value + '=' + o.textContent; });
      check('offers Routing’s rule first, naming its group, then each group', JSON.stringify(options) === JSON.stringify(['=As Routing says (group: personal)', 'personal=Group: personal', 'work=Group: work']), JSON.stringify(options));
      check('follows Routing until picked', select.value === '', select.value);
      pick(select, 'work');
      button('Save', f.closest('form')).click();
      return until(function () { return saved(); }, 5000, 'the save');
    })
    .then(function (cfg) {
      var p = cfg.projects.filter(function (x) { return x.name === 'Northwind'; })[0];
      check('saves the group as the project’s default', p && p.default === 'work', JSON.stringify(p));
      var other = cfg.projects.filter(function (x) { return x.name === 'Atlas'; })[0];
      check('leaves the other project on Routing’s', other && !('default' in other), JSON.stringify(other));
      return until(function () { var c = card('Northwind'); return c && /Group: work/.test(c.textContent) && c; }, 5000, 'the card saying its group');
    })
    .then(function (c) {
      check('the card says Auto’s group', /Auto\s*Group: work/.test(c.textContent), c.textContent);
      button('Routing').click();
      return until(function () { return card('work'); }, 5000, 'the work group’s card in Routing');
    })
    .then(function (c) {
      var before = sent('save_config').length;
      button('Remove', c).click();
      return until(function () { return sent('save_config').length > before && saved(); }, 5000, 'the save after removing the group');
    })
    .then(function (cfg) {
      var p = cfg.projects.filter(function (x) { return x.name === 'Northwind'; })[0];
      check('removing the group puts the project back on Routing’s', p && !('default' in p), JSON.stringify(p));
      check('and the group is gone', !cfg.routing.groups.some(function (g) { return g.name === 'work'; }), JSON.stringify(cfg.routing.groups));
    })
    .then(done)
    .catch(stopped);
})();
