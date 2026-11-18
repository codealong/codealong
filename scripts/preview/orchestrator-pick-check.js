// Checks starting a project's orchestrator from All projects (StartOrchestrator.tsx): with the
// first project's turned off, the compass offers All projects or one of your projects, the dialog
// asks which, starting with All projects, and Start orchestrator starts the one picked.
//   scripts/preview.sh sessions '' scripts/preview/orchestrator-pick-check.js
(function () {
  // All projects, as preview.sh would set it for a project named on its command line, which
  // checks.sh can't give one with a space in it.
  localStorage.setItem('codealong.project', 'All projects');
  window.__ORCHESTRATES__ = { project: '$first', off: true };
  var picked = null;
  say('orchestrator pick check: waiting for the compass');
  until(function () { return q('.ca-listbar-end'); }, 20000, 'the compass in Sessions’ bar')
    .then(function (button) {
      check('under All projects it offers All projects or a project', /All projects or one of your projects/.test(button.title), button.title);
      button.click();
      return until(function () { return q('.ca-startorch select'); }, 5000, 'the dialog’s picker');
    })
    .then(function (select) {
      var options = all('option', select).map(function (o) { return o.textContent; });
      check('it starts on All projects', select.value === '' && options[0] === 'All projects', options.join(' | '));
      check('and lists your projects after it', options.length > 1 && options.indexOf('Elsewhere') < 0, options.join(' | '));
      picked = options[1];
      // Picked as a person picks it: React reads the value as the change comes.
      var set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
      set.call(select, picked);
      select.dispatchEvent(new Event('change', { bubbles: true }));
      return wait(100);
    })
    .then(function () {
      var dialog = q('.ca-startorch');
      check('the dialog follows the pick', dialog.textContent.indexOf('Start an orchestrator for ' + picked + '?') >= 0, dialog.textContent.slice(0, 120));
      all('button', dialog).filter(function (b) { return /Start orchestrator/.test(b.textContent); })[0].click();
      return until(function () { return !q('.ca-startorch'); }, 5000, 'the dialog put away');
    })
    .then(function () {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('it starts the project picked', ops.some(function (o) { return o.op === 'orchestrate' && o.project === picked; }), JSON.stringify(ops));
    })
    .then(done)
    .catch(stopped);
})();
