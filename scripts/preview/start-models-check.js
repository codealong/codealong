// Checks that the quick panel's agent menu lists each account's own models (Quick.tsx,
// start.ts `withLists`): an account whose harness lists some (Claude Code's aliases,
// Antigravity's `agy models`) offers them, one that lists none offers only itself, and a start
// on one asks for it where it opens if that place takes a model (the Codex app, whose thread
// Codealong makes, does), else in the terminal, with the worktree choice left to a harness that
// makes one.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/start-models-check.js
// It needs an account of each kind in config.toml; one that's missing is noted and its steps
// are skipped.
(function () {
  var agentChip = function () { return document.querySelector('.ca-quick-agent .ca-quick-chip'); };
  var options = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-picker-opt')); };
  var listed = null;

  localStorage.setItem('codealong.quick.project', 'All projects');
  say('start models check: waiting for the panel');
  fetch('./start-options.json')
    .then(function (r) { return r.json(); })
    .then(function (o) {
      listed = o.accounts;
      // The panel says it's ready once it has opened, which sets its choices back to none.
      return until(function () { return calls('quick_ready').length > 0 && !!agentChip() && !agentChip().disabled; }, 30000);
    })
    .then(function () {
      key(window, { key: 'l', metaKey: true });
      return until(function () { return options().length > 0; }, 2000);
    })
    .then(function () {
      var text = options().map(function (o) { return o.textContent; });
      listed.forEach(function (a) {
        var models = a.models || [];
        if (a.places.length === 0) return;
        var shown = models.filter(function (m) { return text.some(function (t) { return t.indexOf(m.label.split(':')[0].replace(' (1M context)', ' 1M')) >= 0; }); }).length;
        check(a.name + ' lists its ' + models.length + ' models', shown === models.length, shown + ' shown');
      });
      var other = listed.find(function (a) { return a.harness !== 'claude' && (a.models || []).length > 0 && a.places.length > 0; });
      if (!other) {
        note('no account besides Claude Code lists models here: the rest skipped');
        return null;
      }
      var model = other.models[other.models.length - 1];
      var opt = options().find(function (o) { return o.textContent.indexOf(model.label) >= 0; });
      check('its model can be picked', !!opt && !opt.disabled, model.label);
      opt.click();
      return until(function () { return agentChip().textContent.indexOf(model.label) >= 0; }, 5000).then(function () { return [other, model]; });
    })
    .then(function (picked) {
      if (!picked) return;
      var other = picked[0], model = picked[1];
      check('the chip says the account and model', agentChip().textContent.indexOf(model.label) >= 0, agentChip().textContent);
      var tree = document.querySelector('.ca-quick-newtree input');
      check('a new worktree is offered only where the harness makes one', !!tree && tree.disabled === !other.worktree, tree ? String(tree.disabled) : 'no checkbox');
      var box = document.querySelector('.ca-quick-input textarea');
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, 'Say hello');
      box.dispatchEvent(new Event('input', { bubbles: true }));
      return wait(200).then(function () {
        key(window, { key: 'Enter', metaKey: true });
        return until(function () { return calls('start_from_quick').length > 0; }, 3000).then(function () {
          var spec = calls('start_from_quick')[0][1].spec;
          var takes = function (p) { return ['terminal', 'background', 'codealong', 'codex-app'].indexOf(p) >= 0; };
          var where = takes(other.chosen) ? other.chosen : other.places.map(function (p) { return p.place; }).find(takes);
          check('the start asks for it where it opens, a place that takes one', spec.account === other.name && spec.model === model.id && spec.place === where && !spec.worktree, JSON.stringify({ account: spec.account, model: spec.model, place: spec.place, worktree: spec.worktree, expected: where }));
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
