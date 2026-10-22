// Checks the Schedules view (ui/SchedulesView.tsx, ui/SchedulePanel.tsx, ui/ScheduleDialog.tsx):
// a row for each of the project's schedules saying when it runs, what it does and when next, a
// paused one saying so, and none for a chat of its own; the panel of the one selected, with its
// runs; the row's menu (menus.ts `scheduleMenu`), whose Run now and Pause send their change; and
// the dialog, which previews a time as it's written and saves what it was given, and whose
// session field (FieldPicker.tsx) finds a session by typing, says how long since it did anything,
// and brings its worktree. Under a schedule, set in as helpers are in Sessions, its latest run's
// session and the ones it started, an earlier run's a click away (boardLists.ts
// `scheduleSessions`); one picked there shows its own panel, with the way back to the schedule's.
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

  // The rows set in under schedule `name`, up to the next schedule's.
  var under = function (name) {
    var out = [];
    for (var el = named(name).nextElementSibling; el && !el.classList.contains('ca-schedrow'); el = el.nextElementSibling) out.push(el);
    return out;
  };
  var sessionsUnder = function (name) { return under(name).filter(function (el) { return el.classList.contains('is-nested') && !el.classList.contains('is-fold'); }); };
  var foldUnder = function (name) { return under(name).filter(function (el) { return el.classList.contains('is-fold'); })[0]; };
  var shownCount = 0;
  var moreCount = 0;

  say('schedules check: waiting for the schedules');
  until(function () { return rows().length >= 5; }, 20000, 'the sample schedules')
    .then(function () {
      // The board is this Mac's: its sessions may have started others of their own, so the counts
      // go by what the fold says.
      var listed = sessionsUnder('Morning triage');
      shownCount = listed.length;
      check('under a schedule, its latest run’s session and the two it started at least', listed.length >= 3, listed.map(function (el) { return el.textContent; }).join(' | '));
      var fold = foldUnder('Morning triage');
      var more = fold && /List (\d+) more sessions?/.exec(fold.textContent);
      moreCount = more ? Number(more[1]) : 0;
      check('an earlier run’s, done with, a click away', moreCount >= 1, fold ? fold.textContent : 'none');
      // Return on the button is the button's: it neither changes the schedule nor opens a session.
      var button = fold.querySelector('button');
      button.focus();
      button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      check('Return on it is its own, not the selected schedule’s', !q('.ca-scheddlg'), 'the dialog opened');
      button.click();
      return until(function () { return sessionsUnder('Morning triage').length === shownCount + moreCount; }, 3000, 'every session listed');
    })
    .then(function () {
      check('and back to the latest and those at work', /List only the latest run/.test(foldUnder('Morning triage').textContent), foldUnder('Morning triage').textContent);
      foldUnder('Morning triage').querySelector('button').click();
      return until(function () { return sessionsUnder('Morning triage').length === shownCount; }, 3000, 'the fold');
    })
    .then(function () {
      press(sessionsUnder('Morning triage')[1]);
      return until(function () { return !!q('.ca-panelback'); }, 3000, 'the session’s own panel');
    })
    .then(function () {
      var back = q('.ca-panelback');
      check('a session picked there shows its own panel, the way back naming the schedule', back.textContent.indexOf('Morning triage') >= 0 && !q('.ca-schedinsp'), back.textContent);
      check('it’s the one selected, not its schedule', sessionsUnder('Morning triage')[1].classList.contains('is-selected') && !named('Morning triage').classList.contains('is-selected'), named('Morning triage').className);
      // The window's keys go by it, as by a session selected in a list: ⌘N starts in its worktree.
      var n = sent('show_quick').length;
      q('.ca-schedlist').focus();
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', code: 'KeyN', metaKey: true, bubbles: true, cancelable: true }));
      var asked = sent('show_quick').slice(n)[0];
      check('⌘N starts a session in its worktree', !!asked && !!asked.context && !!asked.context.laneId, JSON.stringify(asked));
      back.click();
      return until(function () { return !!q('.ca-schedinsp'); }, 3000, 'the schedule’s panel again');
    })
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
      var where = q('.ca-scheddlg .ca-fieldpick-btn');
      check('a new one starts in the project’s worktree', where.textContent.indexOf('No project') < 0, where.textContent);
      where.click();
      return until(function () { return q('.ca-fieldpick .ca-picker-filter'); }, 3000, 'the worktree menu');
    })
    .then(function () {
      // No project's id is empty, and ⏎ still picks it.
      var filter = q('.ca-fieldpick .ca-picker-filter');
      type(filter, 'no project');
      return wait(50).then(function () {
        filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        return until(function () { return !q('.ca-fieldpick .ca-picker'); }, 3000, 'the worktree menu to close');
      });
    })
    .then(function () {
      var where = q('.ca-scheddlg .ca-fieldpick-btn');
      check('⏎ on No project makes it a chat of its own', where.textContent.indexOf('No project') >= 0, where.textContent);
      return wait(50);
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
      check('in no worktree, as it was picked', s.run && s.run.folder === '', JSON.stringify(s.run));
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
      all('.ca-scheddlg .ca-fieldpick-btn')[1].click();
      return until(function () { return q('.ca-fieldpick .ca-picker'); }, 3000, 'the worktree menu');
    })
    .then(function () {
      // A press on the other field's button closes this menu as it opens that one.
      var field = q('.ca-fieldpick-btn[aria-label="The session it sends to"]');
      press(field);
      field.click();
      return until(function () { return q('.ca-fieldpick .ca-picker-opt .ca-picker-trail'); }, 3000, 'the sessions');
    })
    .then(function () {
      check('one menu is open at a time', all('.ca-picker').length === 1, all('.ca-picker').length + ' open');
      return wait(10);
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
