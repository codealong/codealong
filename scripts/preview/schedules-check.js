// Checks the Schedules view (ui/SchedulesView.tsx, ui/SchedulePanel.tsx, ui/ScheduleDialog.tsx):
// a row for each of the project's schedules saying when it runs, what it does and when next, a
// paused one saying so, and none for a chat of its own; the panel of the one selected, with its
// runs; the row's menu (menus.ts `scheduleMenu`), whose Run now and Pause send their change; and
// the dialog, which previews a time as it's written and saves what it was given, and whose
// session field (FieldPicker.tsx) finds a session by typing, says how long since it did anything,
// and brings its worktree.
//   PREVIEW_WAIT=60000 PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules '' scripts/preview/schedules-check.js
(function () {
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-schedrow')); };
  var named = function (name) { return rows().filter(function (r) { return r.querySelector('.ca-schedrow-name').textContent === name; })[0]; };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  var type = function (input, value) {
    var set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), 'value').set;
    set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var bare = function (l) { return l.replace(/ ✓$/, ''); };

  say('schedules check: waiting for the schedules');
  until(function () { return rows().length >= 5; }, 20000, 'the sample schedules')
    .then(function () {
      var triage = named('Morning triage');
      check('a row says when it runs', triage.textContent.indexOf('Weekdays at 9:00') >= 0, triage.textContent);
      check('and what it does', !!triage.querySelector('.ca-schedrow-does') && triage.querySelector('.ca-schedrow-does').textContent.indexOf('New session in') === 0, triage.textContent);
      check('and when it runs next', /Next in/.test(triage.querySelector('.ca-schedrow-next').textContent), triage.textContent);
      var paused = named('Weekly dependency check');
      check('a paused one says so', paused.classList.contains('is-paused') && paused.querySelector('.ca-schedrow-next').textContent === 'Paused', paused.textContent);
      var failed = named('Release notes');
      check('one whose last run failed says so', failed.textContent.indexOf('Last run failed') >= 0, failed.textContent);
      check('a chat of its own is in no project, so only All projects lists it', !named('Standup notes'), rows().map(function (r) { return r.querySelector('.ca-schedrow-name').textContent; }).join(' | '));
      check('the sidebar counts the ones that run', /Schedules\s*4/.test(q('.ca-side').textContent.replace(/\s+/g, ' ')), q('.ca-side').textContent);
      press(named('After each deploy'));
      return until(function () { return q('.ca-schedinsp-name') && q('.ca-schedinsp-name').textContent === 'After each deploy'; }, 3000, 'its panel');
    })
    .then(function () {
      var panel = q('.ca-schedinsp');
      check('its panel says what it watches', !!panel.querySelector('.ca-schedinsp-code') && panel.textContent.indexOf('Last look') >= 0, panel.textContent);
      check('and its runs', panel.querySelectorAll('.ca-schedrun').length === 2, panel.textContent);
      check('and that it keeps one session going', panel.textContent.indexOf('Keeps one session going') >= 0, panel.textContent);
      return menuOf(named('After each deploy'));
    })
    .then(function (labels) {
      note('menu: ' + labels.join(' | '));
      check('its menu runs, changes, pauses and deletes it', ['Run now', 'Edit…', 'Pause', 'Delete schedule'].every(function (l) { return labels.map(bare).indexOf(l) >= 0; }), labels.join(' | '));
      var n = sent('schedule_op').length;
      menuItem('Run now')();
      return until(function () { return sent('schedule_op').length > n; }, 3000, 'Run now');
    })
    .then(function () {
      var op = last('schedule_op').op;
      check('Run now asks for a run', op.op === 'run' && op.id === 's-1', JSON.stringify(op));
      return menuOf(named('After each deploy'));
    })
    .then(function () {
      var n = sent('schedule_op').length;
      menuItem('Pause')();
      return until(function () { return sent('schedule_op').length > n; }, 3000, 'Pause');
    })
    .then(function () {
      var op = last('schedule_op').op;
      check('Pause pauses it', op.op === 'pause' && op.paused === true, JSON.stringify(op));
      q('.ca-schednew').click();
      return until(function () { return q('.ca-scheddlg'); }, 3000, 'the dialog');
    })
    .then(function () {
      type(q('.ca-scheddlg input[placeholder="Morning triage"]'), 'Nightly review');
      type(q('.ca-scheddlg input[aria-label="Times"]'), 'every 2h');
      return until(function () { return /every 2h\. Next:/.test(q('.ca-scheddlg').textContent); }, 3000, 'the time read back');
    })
    .then(function () {
      check('the time is read back as it’s written', true);
      var box = q('.ca-scheddlg textarea[aria-label="What it\'s told each run"]');
      type(box, 'Review what merged today.');
      box.dispatchEvent(new FocusEvent('blur'));
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
      return wait(100);
    })
    .then(function () {
      var n = sent('schedule_op').length;
      q('.ca-scheddlg button[type="submit"]').click();
      return until(function () { return sent('schedule_op').length > n; }, 3000, 'the save');
    })
    .then(function () {
      var op = last('schedule_op').op;
      var s = op.schedule || {};
      check('saving sends the schedule', op.op === 'save' && s.name === 'Nightly review' && s.when.kind === 'time' && s.when.rule === 'every 2h', JSON.stringify(op));
      check('with what it’s told', s.run && s.run.prompt === 'Review what merged today.', JSON.stringify(s.run));
      check('in the project’s worktree, as the view had it', s.run && s.run.folder !== '', JSON.stringify(s.run));
      return until(function () { return !q('.ca-scheddlg'); }, 3000, 'the dialog to close');
    })
    .then(function () {
      q('.ca-schednew').click();
      return until(function () { return q('.ca-scheddlg'); }, 3000, 'the dialog again');
    })
    .then(function () {
      type(q('.ca-scheddlg input[placeholder="Morning triage"]'), 'Nudge');
      all('.ca-scheddlg .ca-segctl button').filter(function (b) { return b.textContent === 'A session'; })[0].click();
      return until(function () { return q('.ca-fieldpick-btn[aria-label="The session it sends to"]'); }, 3000, 'the session field');
    })
    .then(function () {
      var field = q('.ca-fieldpick-btn[aria-label="The session it sends to"]');
      check('the session field starts empty', field.textContent.indexOf('Pick a session') >= 0, field.textContent);
      field.click();
      return until(function () { return q('.ca-fieldpick .ca-picker-opt'); }, 3000, 'the sessions');
    })
    .then(function () {
      var opts = all('.ca-fieldpick .ca-picker-opt');
      check('it lists sessions with how long since each did anything', opts.length > 1 && opts.every(function (o) { return !!o.querySelector('.ca-picker-trail'); }), opts.length + ' options');
      var worked = opts.filter(function (o) { return o.querySelector('.ca-picker-detail').textContent.indexOf('No project') !== 0; })[1];
      if (!worked) throw new Error('no session in a worktree to pick');
      var title = worked.querySelector('.ca-picker-title').textContent;
      var detail = worked.querySelector('.ca-picker-detail').textContent;
      note('picking: ' + title + ' (' + detail + ')');
      var filter = q('.ca-fieldpick .ca-picker-filter');
      type(filter, 'zzqx nothing like it');
      check('a filter that finds nothing says so', /No session matches/.test(q('.ca-fieldpick .ca-picker').textContent), q('.ca-fieldpick .ca-picker').textContent);
      type(filter, title);
      return wait(50).then(function () {
        var shown = all('.ca-fieldpick .ca-picker-opt');
        var at = shown.findIndex(function (o) { return o.querySelector('.ca-picker-title').textContent === title && o.querySelector('.ca-picker-detail').textContent === detail; });
        check('typing its title narrows the list to it', shown.length < opts.length && at >= 0, shown.length + ' of ' + opts.length);
        // ↓ to it, past any other session its title's words find, then ⏎.
        for (var i = 0; i < at; i++) filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
        return wait(50);
      }).then(function () {
        filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        return until(function () { return !q('.ca-fieldpick .ca-picker'); }, 3000, 'the menu to close');
      }).then(function () { return wait(50); }).then(function () { return title; });
    })
    .then(function (title) {
      var field = q('.ca-fieldpick-btn[aria-label="The session it sends to"]');
      check('⏎ picks it, and the field says it', field.textContent.indexOf(title) >= 0 && document.activeElement === field, field.textContent);
      var where = all('.ca-scheddlg .ca-fieldpick-btn')[1];
      check('and brings its worktree', !!where && where.textContent.indexOf('No worktree') < 0 && where.textContent.indexOf('In') === 0, where ? where.textContent : 'none');
      type(q('.ca-scheddlg textarea[aria-label="What it sends"]'), 'Carry on.');
      var n = sent('schedule_op').length;
      q('.ca-scheddlg button[type="submit"]').click();
      return until(function () { return sent('schedule_op').length > n; }, 3000, 'the save');
    })
    .then(function () {
      var run = (last('schedule_op').op.schedule || {}).run || {};
      check('saving sends to that session, in its worktree', run.to === 'session' && run.session !== '' && run.folder !== '', JSON.stringify(run));
      return until(function () { return !q('.ca-scheddlg'); }, 3000, 'the dialog to close');
    })
    .then(done)
    .catch(stopped);
})();
