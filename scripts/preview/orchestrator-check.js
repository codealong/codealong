// Checks an orchestrator's session in Sessions (orchestrators.ts): it leads the list in a section
// of its own, is listed once, never among the rest, and the bar offers no compass while it runs;
// its menu looks now, starts it afresh and turns it off, which pauses its schedule (its scheduler
// then marks its session done: runner.rs `Job::Retire`).
//   scripts/preview.sh sessions '' scripts/preview/orchestrator-check.js
(function () {
  // The first project's, which the view shows.
  window.__ORCHESTRATES__ = { project: '$first' };
  var keyOf = function (row) { return row.id.slice(row.id.indexOf('::') + 2); };
  say('orchestrator check: waiting for Sessions');
  until(function () { return q('.ca-orchestrators [id^="row-"]'); }, 20000, 'the orchestrator section')
    .then(function (row) {
      var key = keyOf(row);
      var section = q('.ca-orchestrators');
      var list = q('.ca-sessionlist');
      check('it leads the list', !list || !!(section.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING), 'the list comes before it');
      check('its section is called Orchestrator', /Orchestrator/.test(q('.ca-bay', section).textContent), q('.ca-bay', section).textContent);
      var rows = all('[id^="row-"]').filter(function (r) { return keyOf(r) === key; });
      check('it is listed once', rows.length === 1, rows.length + ' rows');
      check('the bar offers no compass while it runs', !q('.ca-listbar-start'));
      rightClick(row);
      return wait(100).then(function () { return { row: row, key: key }; });
    })
    .then(function (it) {
      var labels = menuLabels();
      ['Look now', 'Start afresh', 'Turn off orchestrator'].forEach(function (l) { check('its menu has ' + l, labels.indexOf(l) >= 0, labels.join(' | ')); });
      menuItem('Turn off orchestrator')();
      return until(function () { return calls('schedule_op').length > 0; }, 5000, 'its schedule changed').then(function () { return it; });
    })
    .then(function () {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('turning it off pauses its schedule', ops.some(function (o) { return o.op === 'pause' && /^orch-/.test(o.id) && o.paused === true; }), JSON.stringify(ops));
      rightClick(q('.ca-orchestrators [id^="row-"]'));
      return wait(100);
    })
    .then(function () {
      menuItem('Start afresh')();
      return until(function () { return sent('schedule_op').some(function (a) { return a.op.op === 'fresh'; }); }, 5000, 'a fresh run asked for');
    })
    .then(function () {
      check('starting afresh asks its schedule for a fresh run', sent('schedule_op').some(function (a) { return a.op.op === 'fresh' && /^orch-/.test(a.op.id); }));
    })
    .then(done)
    .catch(stopped);
})();
