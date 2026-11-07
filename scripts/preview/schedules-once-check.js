// Checks what the Schedules view shows of schedules that run once and of the harnesses' own
// (ui/SchedulesView.tsx, ui/SchedulePanel.tsx, ui/ScheduleDialog.tsx): one that runs once says so;
// a finished one is folded under Finished, out of the sidebar's count, and shown with its tag and
// why; the dialog's Run once, then stop is saved; the schedules the harnesses keep are listed apart,
// read only, each with the session it wakes, a panel of its own, and Return opening it where it
// lives (`open_harness_link`); one that can't run (a Codex heartbeat whose thread is gone) says so,
// on its row and in its panel, and its Open in Codex opens it on its own page.
// It runs under All projects, where a harness's schedule whose place can't be told (a heartbeat
// whose session isn't on the board) is listed too.
//   PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules 'All projects' scripts/preview/schedules-once-check.js
(function () {
  var rows = function () { return all('.ca-schedrow'); };
  var named = function (name) { return rows().filter(function (r) { return r.querySelector('.ca-schedrow-name').textContent === name; })[0]; };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  var heading = function (label) { return all('.ca-schedlist .ca-bay--section').filter(function (h) { return h.textContent.indexOf(label) === 0; })[0]; };

  say('schedules once check: waiting for the schedules');
  until(function () { return rows().length >= 5 && !!heading('Kept by their harnesses'); }, 20000, 'the sample schedules and the harnesses’ own')
    .then(function () {
      var once = named('Once the atlas deploy is out');
      check('one that runs once says so', !!once && !!once.querySelector('.ca-schedrow-once'), once ? once.textContent : 'no row');
      var finished = heading('Finished');
      check('the finished ones are folded under Finished, counted', !!finished && /Finished\s*1/.test(finished.textContent) && !named('Follow up on the kestrel release'), finished ? finished.textContent : 'no heading');
      // The sidebar counts the ones that will run: listed, Codealong's own, neither paused nor finished.
      var running = rows().filter(function (r) { return !r.classList.contains('is-harness') && !r.classList.contains('is-paused'); }).length;
      check('and left out of the sidebar’s count', new RegExp('Schedules\\s*' + running + '\\b').test(q('.ca-side').textContent.replace(/\s+/g, ' ')), running + ' running; ' + q('.ca-side').textContent);
      finished.querySelector('button').click();
      return until(function () { return !!named('Follow up on the kestrel release'); }, 3000, 'the finished one listed');
    })
    .then(function () {
      var row = named('Follow up on the kestrel release');
      check('listed, it has nothing next', !row.querySelector('.ca-schedrow-next') && row.classList.contains('is-paused'), row.textContent);
      press(row);
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'Follow up on the kestrel release'; }, 3000, 'its panel');
    })
    .then(function () {
      var panel = q('.ca-schedinsp');
      check('its panel says it’s finished, and why', panel.querySelector('.ca-tag').textContent === 'Finished' && /Finished .*it ran once, as it was set to/.test(panel.textContent), panel.textContent.slice(0, 300));
      press(named('Once the atlas deploy is out'));
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'Once the atlas deploy is out'; }, 3000, 'its panel');
    })
    .then(function () {
      var panel = q('.ca-schedinsp');
      check('its panel says it stops after its first run', panel.textContent.indexOf('Stops after its first run. Run now doesn’t count.') >= 0, panel.textContent.slice(0, 400));
      check('and that a new session takes its session’s place if it’s gone', panel.textContent.indexOf('If that session is gone, a new one starts in its place') >= 0, panel.textContent.slice(0, 400));
      all('.ca-insp-actions button').filter(function (b) { return b.textContent === 'Edit'; })[0].click();
      return until(function () { return q('.ca-scheddlg'); }, 3000, 'its dialog');
    })
    .then(function () {
      var box = all('.ca-scheddlg .ca-check').filter(function (l) { return l.textContent === 'Run once, then stop'; })[0];
      check('its dialog has Run once, then stop, ticked', !!box && box.querySelector('input:checked') !== null, box ? box.outerHTML.slice(0, 300) : 'no box');
      var n = sent('schedule_op').length;
      q('.ca-scheddlg button[type="submit"]').click();
      return until(function () { return sent('schedule_op').length > n; }, 3000, 'the save');
    })
    .then(function () {
      var s = last('schedule_op').op.schedule || {};
      check('saving keeps it running once', s.once === true && s.name === 'Once the atlas deploy is out', JSON.stringify({ once: s.once, name: s.name }));
      return until(function () { return !q('.ca-scheddlg'); }, 3000, 'the dialog to close');
    })
    .then(function () {
      var kept = named('Check the northwind deploy and report');
      check('the harnesses’ own are listed apart, read only, saying whose', !!kept && kept.classList.contains('is-harness') && kept.textContent.indexOf('Claude Code cron job') >= 0 && /Next in/.test(kept.textContent), kept ? kept.textContent : 'no row');
      var codex = named('Nightly atlas triage');
      check('a paused one says so', !!codex && codex.querySelector('.ca-schedrow-next').textContent === 'Paused', codex ? codex.textContent : 'no row');
      var wakes = kept && kept.nextElementSibling;
      check('with the session it wakes set in under it', !!wakes && wakes.classList.contains('is-nested'), wakes ? wakes.className : 'none');
      press(kept);
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'Check the northwind deploy and report'; }, 3000, 'its panel');
    })
    .then(function () {
      var panel = q('.ca-schedinsp');
      check('its panel says the harness keeps it, and what keeps it going', panel.textContent.indexOf('Claude Code keeps it, not Codealong') >= 0 && panel.textContent.indexOf('Only while its session') >= 0, panel.textContent.slice(0, 400));
      check('and the session it wakes', !!panel.querySelector('.ca-sessinsp-kin'), panel.textContent.slice(0, 400));
      press(named('Nightly atlas triage'));
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'Nightly atlas triage'; }, 3000, 'the Codex one’s panel');
    })
    .then(function () {
      var open = all('.ca-insp-actions button').filter(function (b) { return b.textContent === 'Open in Codex'; })[0];
      check('one with a link opens in its app', !!open, q('.ca-insp-actions') ? q('.ca-insp-actions').textContent : 'no actions');
      var n = sent('open_harness_link').length;
      q('.ca-schedlist').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      return until(function () { return sent('open_harness_link').length > n; }, 3000, 'the open');
    })
    .then(function () {
      var asked = last('open_harness_link');
      check('Return opens it there, by its link', asked.harness === 'codex' && asked.url === 'codex://automations/nightly-atlas-triage?automationSource=local', JSON.stringify(asked));
      check('and opens no dialog', !q('.ca-scheddlg'), '');
      var gone = named('Watch the kestrel rollout');
      check('one that can’t run says so on its row', !!gone && gone.classList.contains('is-problem') && !!gone.querySelector('.ca-schedrow-problem') && gone.querySelector('.ca-schedrow-problem').textContent === 'Can’t run', gone ? gone.textContent : 'no row');
      check('with no next time', !!gone && !gone.querySelector('.ca-schedrow-next'), gone ? gone.textContent : 'no row');
      press(gone);
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'Watch the kestrel rollout'; }, 3000, 'its panel');
    })
    .then(function () {
      var panel = q('.ca-schedinsp');
      check('its panel says why it can’t run', panel.querySelector('.ca-tag').textContent === 'Can’t run' && !!panel.querySelector('.ca-schedproblem') && panel.querySelector('.ca-schedproblem').textContent.indexOf('Its thread is gone from Codex') >= 0, panel.textContent.slice(0, 400));
      check('and not that its session isn’t on the board', panel.textContent.indexOf('isn’t on the board') < 0, panel.textContent.slice(0, 600));
      var open = all('.ca-insp-actions button').filter(function (b) { return b.textContent === 'Open in Codex'; })[0];
      var n = sent('open_harness_link').length;
      open.click();
      return until(function () { return sent('open_harness_link').length > n; }, 3000, 'the open');
    })
    .then(function () {
      check('its Open in Codex opens it on its own page', last('open_harness_link').url === 'codex://automations/watch-kestrel-rollout?automationSource=local', JSON.stringify(last('open_harness_link')));
    })
    .then(done, stopped);
})();
