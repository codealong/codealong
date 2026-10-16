// Checks that a Codex account offers its own effort levels in the quick panel's agent menu
// (start.ts `startChoice`, StartControls.tsx `EffortFoot`): with one of its models chosen, the
// row under the models runs Low to Ultra, fits the menu, and doesn't say Codex picks its own; →
// steps to Ultra, the chip says it, and a start in the Codex app asks for the model and the level.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/codex-effort-check.js
// It needs a Codex account that lists models (one Codex has run in); without one it's noted.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var agentChip = function () { return document.querySelector('.ca-quick-agent .ca-quick-chip'); };
  var filter = function () { return document.querySelector('.ca-picker-filter'); };
  var options = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-picker-opt')); };
  var foot = function () { return document.querySelector('.ca-picker-foot'); };
  var levels = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-picker-foot [role="radio"], .ca-picker-foot button')).map(function (b) { return b.textContent; }); };
  var effortOn = function () { var b = document.querySelector('.ca-picker-foot [aria-checked="true"]'); return b ? b.textContent : null; };
  var codex = null;
  var model = null;

  say('codex effort check: waiting for the panel');
  fetch('./start-options.json')
    .then(function (r) { return r.json(); })
    .then(function (o) {
      codex = o.accounts.find(function (a) { return a.harness === 'codex' && (a.models || []).length > 0 && (a.efforts || []).length > 0 && a.places.length > 0; });
      return until(function () { return calls('quick_ready').length > 0 && !!agentChip() && !agentChip().disabled; }, 30000);
    })
    .then(function () {
      if (!codex) {
        note('no Codex account lists models here: nothing to check');
        return null;
      }
      model = codex.models[1] || codex.models[0];
      key(window, { key: 'l', metaKey: true });
      return until(function () { return options().length > 0; }, 2000).then(function () {
        var opt = options().find(function (o) { return o.textContent.indexOf(codex.name) >= 0 && o.textContent.indexOf(model.label) >= 0; }) || options().find(function (o) { return o.textContent.indexOf(model.label) >= 0; });
        check('its model can be picked', !!opt && !opt.disabled, model.label);
        opt.click();
        return until(function () { return agentChip().textContent.indexOf(model.label) >= 0; }, 3000);
      });
    })
    .then(function (go) {
      if (go === null) return null;
      key(window, { key: 'l', metaKey: true });
      return until(function () { return !!foot(); }, 2000).then(function () {
        var shown = levels();
        var want = ['Default'].concat(codex.efforts.map(function (e) { return e.label; }));
        check('the effort row runs Default, then its own levels', want.every(function (l) { return shown.indexOf(l) >= 0; }) && shown.indexOf('Ultra') >= 0, shown.join(' | '));
        var f = foot();
        check('the row fits the menu', f.scrollWidth <= f.clientWidth + 1 && f.getBoundingClientRect().right <= document.querySelector('.ca-picker').getBoundingClientRect().right + 1, f.scrollWidth + ' in ' + f.clientWidth);
        check('it doesn’t say Codex picks its own', f.textContent.indexOf('picks its own') < 0, f.textContent);
        for (var i = 0; i < 8; i++) key(filter(), { key: 'ArrowRight' });
        return wait(100);
      });
    })
    .then(function (go) {
      if (go === null) return null;
      check('→ steps up to Ultra and stops there', effortOn() === 'Ultra', effortOn());
      key(filter(), { key: 'Enter' });
      return until(function () { return !filter(); }, 2000).then(function () {
        check('the chip says it', /Ultra effort/.test(agentChip().textContent), agentChip().textContent);
        var box = document.querySelector('.ca-quick-input textarea');
        Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, 'Say hello');
        box.dispatchEvent(new Event('input', { bubbles: true }));
        return wait(200);
      });
    })
    .then(function (go) {
      if (go === null) return null;
      key(window, { key: 'Enter', metaKey: true });
      return until(function () { return calls('start_from_quick').length > 0; }, 3000).then(function () {
        var spec = calls('start_from_quick')[0][1].spec;
        check('the start asks for the model and Ultra', spec.account === codex.name && spec.model === model.id && spec.effort === 'ultra', JSON.stringify({ account: spec.account, model: spec.model, effort: spec.effort, place: spec.place }));
      });
    })
    .then(done)
    .catch(stopped);
})();
