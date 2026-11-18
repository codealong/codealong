// Checks an orchestrator's session in Sessions (orchestrators.ts): it leads the list in a section
// of its own, is listed once, never among the rest, and the bar offers no compass while it runs;
// selected, its header says it's the orchestrator for its project, and its conversation folds the
// runs that found nothing (a dash for a reply) into a line each, runs in a row into one
// (orchestrators.ts `foldQuietRuns`), leaving the run that told you something; its menu looks now, starts it
// afresh and turns it off in Mark as done's place, which pauses its schedule (its scheduler then
// marks its session done: runner.rs `Job::Retire`), with no pin, snooze or archive.
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
      check('the bar offers no compass while it runs', !q('.ca-listbar-end'));
      row.click();
      return until(function () { return q('.ca-sessinsp-kin.is-orchestrator'); }, 5000, 'its header’s orchestrator line').then(function (line) { return { row: row, key: key, line: line }; });
    })
    .then(function (it) {
      check('its header says it’s the orchestrator for its project', /^Orchestrator for\s*\S/.test(it.line.textContent.trim()), it.line.textContent);
      check('and not that it came from a schedule', !q('.ca-sessinsp-head') || q('.ca-sessinsp-head').textContent.indexOf('From the schedule') < 0);
      return until(function () { return all('.ca-thread-note').length >= 2 ? all('.ca-thread-note') : null; }, 5000, 'its conversation’s folded runs').then(function (notes) {
        var said = notes.map(function (x) { return x.textContent.trim(); });
        check('its two quiet runs in a row fold into one line', said[0] === 'Looked 2 times: nothing new', said.join(' | '));
        check('and the last one into another', said[said.length - 1] === 'Looked: nothing new', said.join(' | '));
        var replies = all('.ca-thread-agent').map(function (x) { return x.textContent.trim(); });
        check('the run that told you something stays', replies.some(function (t) { return /Two sessions work in one worktree/.test(t); }), replies.join(' | '));
        check('and no dash is drawn', !replies.some(function (t) { return /^[—–-]$/.test(t); }), replies.join(' | '));
        rightClick(it.row);
        return wait(100).then(function () { return it; });
      });
    })
    .then(function (it) {
      var labels = menuLabels();
      ['Look now', 'Start afresh', 'Turn off orchestrator'].forEach(function (l) { check('its menu has ' + l, labels.indexOf(l) >= 0, labels.join(' | ')); });
      ['Mark as done', 'Archive', 'Pin', 'Snooze ›'].forEach(function (l) { check('its menu has no ' + l, labels.indexOf(l) < 0, labels.join(' | ')); });
      menuItem('Turn off orchestrator')();
      return until(function () { return calls('schedule_op').length > 0; }, 5000, 'its schedule changed').then(function () { return it; });
    })
    .then(function () {
      var ops = sent('schedule_op').map(function (a) { return a.op; });
      check('turning it off pauses its schedule', ops.some(function (o) { return o.op === 'pause' && /^orch-/.test(o.id) && o.paused === true; }), JSON.stringify(ops));
      check('and doesn’t mark it done itself', sent('session_done').length === 0, JSON.stringify(sent('session_done')));
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
