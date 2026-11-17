// Checks starting an orchestrator from Sessions' bar (StartOrchestrator.tsx): with the view's one
// turned off, the compass is there and names what it keeps watch over, a click asks first, and
// Start orchestrator asks the engine to start it (`orchestrate`, for All projects with no project)
// and puts the dialog away; with none, nothing leads the list.
//   scripts/preview.sh sessions 'All projects' scripts/preview/orchestrator-start-check.js
(function () {
  window.__ORCHESTRATES__ = { off: true };
  say('orchestrator start check: waiting for the compass');
  until(function () { return q('.ca-listbar-start'); }, 20000, 'the compass in Sessions’ bar')
    .then(function (button) {
      check('it names what it keeps watch over', /All projects/.test(button.title), button.title);
      check('nothing leads the list while it’s off', !q('.ca-orchestrators'));
      button.click();
      return until(function () { return q('.ca-startorch'); }, 5000, 'the dialog');
    })
    .then(function (dialog) {
      check('the dialog asks for All projects', /All projects/.test(dialog.textContent), dialog.textContent.slice(0, 120));
      var start = all('button', dialog).filter(function (b) { return /Start orchestrator/.test(b.textContent); })[0];
      start.click();
      return until(function () { return !q('.ca-startorch'); }, 5000, 'the dialog put away');
    })
    .then(function () {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('it asks the engine to start it', ops.some(function (o) { return o.op === 'orchestrate' && o.project == null; }), JSON.stringify(ops));
    })
    .then(done)
    .catch(stopped);
})();
