// Checks starting an orchestrator from Sessions' bar (StartOrchestrator.tsx): with the view's one
// turned off, the compass is there and names what it keeps watch over, a click asks first, and
// Start orchestrator asks the engine to start it (`orchestrate`, for the view's project) and puts
// the dialog away; while it's off, nothing leads the list.
//   scripts/preview.sh sessions '' scripts/preview/orchestrator-start-check.js
(function () {
  window.__ORCHESTRATES__ = { project: '$first', off: true };
  var scope = null;
  say('orchestrator start check: waiting for the compass');
  until(function () { return q('.ca-listbar-start'); }, 20000, 'the compass in Sessions’ bar')
    .then(function (button) {
      var named = /^Start an orchestrator for (.+?):/.exec(button.title);
      scope = named && named[1];
      check('it names what it keeps watch over', !!scope && scope !== 'All projects', button.title);
      check('nothing leads the list while it’s off', !q('.ca-orchestrators'));
      button.click();
      return until(function () { return q('.ca-startorch'); }, 5000, 'the dialog');
    })
    .then(function (dialog) {
      check('the dialog asks for that project', dialog.textContent.indexOf('Start an orchestrator for ' + scope + '?') >= 0, dialog.textContent.slice(0, 120));
      var start = all('button', dialog).filter(function (b) { return /Start orchestrator/.test(b.textContent); })[0];
      start.click();
      return until(function () { return !q('.ca-startorch'); }, 5000, 'the dialog put away');
    })
    .then(function () {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('it asks the engine to start it for that project', ops.some(function (o) { return o.op === 'orchestrate' && o.project === scope; }), JSON.stringify(ops));
    })
    .then(done)
    .catch(stopped);
})();
