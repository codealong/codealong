// Checks an orchestrator's session in Sessions (orchestrators.ts): it leads the list in a section
// of its own, is listed once, never among the rest, and the bar offers no compass while it runs;
// its menu looks now, starts it afresh and turns it off, which pauses its schedule and marks its
// session done.
//   scripts/preview.sh sessions 'All projects' scripts/preview/orchestrator-check.js
(function () {
  window.__ORCHESTRATES__ = {};
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
      check('the bar offers no compass while it runs', !q('.ca-listtools-start'));
      rightClick(row);
      return wait(100).then(function () { return { row: row, key: key }; });
    })
    .then(function (it) {
      var labels = menuLabels();
      ['Look now', 'Start afresh', 'Turn off orchestrator'].forEach(function (l) { check('its menu has ' + l, labels.indexOf(l) >= 0, labels.join(' | ')); });
      menuItem('Turn off orchestrator')();
      return until(function () { return calls('session_done').length > 0; }, 5000, 'its session marked done').then(function () { return it; });
    })
    .then(function (it) {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('turning it off pauses its schedule', ops.some(function (o) { return o.op === 'pause' && o.id === 'orch-all' && o.paused === true; }), JSON.stringify(ops));
      var done = sent('session_done');
      check('and marks its session done', done.some(function (a) { return a.key === it.key; }), JSON.stringify(done));
    })
    .then(done)
    .catch(stopped);
})();
