// Checks Rewind to this question (SessionThread.tsx, EditFrom.tsx `RewindDialog`, QuestionList.tsx
// `QuestionSteps`) in Sessions' panel: a right-click on your answers to the questions a call asked
// offers Rewind to these questions…, which opens a dialog asking them again one at a time, with
// your answers picked. A new pick and ⌘⏎ ask for a fork from before your answers
// (`fork_session` with their cursor as `from` and your new answers as the prompt) and close the
// dialog; a question left with no answer has Next take you to it first. The conversation is a
// stand-in for the first Claude Code session.
//   scripts/preview.sh sessions '' scripts/preview/rewind-question-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to show'); return; }
  var sessionKey = picked.key;
  var at = picked.lastActivity || Date.now();
  var said = function (kind, text, cursor, more) { return Object.assign({ at: at - (500 - cursor) * 1000, kind: kind, text: text, failed: false, cursor: cursor, n: 0 }, more || {}); };
  var size = { question: 'Which size?', header: 'Size', options: [{ label: 'Small' }, { label: 'Large', description: 'Room for two' }] };
  var days = { question: 'Which days?', header: 'Days', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }] };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'session_page' && args && args.key === sessionKey) {
      return Promise.resolve({ before: null, after: 500, items: [
        said('you', 'Book the room', 100),
        said('tool', 'AskUserQuestion', 200, { id: 't1', questions: [size, days] }),
        said('result', 'The user answered.', 300, { id: 't1', answers: { 'Which size?': ['Large'], 'Which days?': ['Mon', 'only mornings'] } }),
        said('agent', 'Booked the large room for Monday mornings.', 400),
      ] });
    }
    if (cmd === 'fork_session') {
      window.__calls.push([cmd, args]);
      return Promise.resolve({ key: null, message: 'Forked from before that message.' });
    }
    return real.apply(this, arguments);
  };
  var answers = function () { return all('.ca-thread-said.is-you').filter(function (s) { return /Size/.test(s.textContent) && /Days/.test(s.textContent); })[0]; };
  var dialog = function () { return q('.ca-rewinddlg'); };
  var choice = function (label) { return all('.ca-rewinddlg .ca-question-choice').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0]; };
  var on = function (label) { var c = choice(label); return !!c && c.getAttribute('aria-checked') === 'true'; };
  var shown = function () { return q('.ca-rewinddlg .ca-question-body .ca-approval-label').textContent; };
  var primary = function () { return all('.ca-rewinddlg .ca-dialog-foot button').filter(function (b) { return /is-primary|ca-btn-primary/.test(b.className) || b.textContent === 'Next' || b.textContent === 'Fork and answer'; }).pop(); };

  (async function () {
    try {
      say('rewind check: opening a conversation');
      await until(function () { return q('[id$="::' + sessionKey + '"]'); }, 20000, 'the session isn’t listed');
      q('[id$="::' + sessionKey + '"]').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await until(answers, 20000, 'no answers in the conversation');
      var menu = await menuOf(answers());
      check('your answers offer Rewind to these questions…', menu.indexOf('Rewind to these questions…') >= 0, menu.join(' | '));
      check('and no Edit from here', menu.indexOf('Edit from here…') < 0, menu.join(' | '));
      menuItem('Rewind to these questions…')();
      await until(dialog, 3000, 'no dialog');
      check('it opens a dialog named for them', q('.ca-rewinddlg h2').textContent === 'Rewind to these questions', q('.ca-rewinddlg h2').textContent);
      check('which says the session and the files stay', /stays as it is, and so do the files/.test(q('.ca-rewinddlg .ca-dialog-lede').textContent));
      check('asking the first question, with a strip to the other', /Which size\?/.test(shown()) && all('.ca-rewinddlg .ca-question-steps button').length === 2, shown());
      check('your answer to it picked', on('Large') && !on('Small'));
      check('every question answered, so it can fork at once', primary().textContent === 'Fork and answer' && !primary().disabled, primary().textContent);
      choice('Small').click();
      await wait(50);
      check('a new pick takes the old one’s place', on('Small') && !on('Large'));
      all('.ca-rewinddlg .ca-question-steps button')[1].click();
      await wait(50);
      check('the other question has your picks and your own words', on('Mon') && !on('Tue') && q('.ca-rewinddlg .ca-question-own').value === 'only mornings', q('.ca-rewinddlg .ca-question-own').value);
      choice('Tue').click();
      await wait(50);
      key(choice('Tue'), { key: 'Enter', metaKey: true });
      await until(function () { return calls('fork_session').length === 1; }, 3000, 'no fork asked for');
      var asked = calls('fork_session')[0][1];
      check('⌘⏎ forks from before your answers', asked.key === sessionKey && asked.from === 300, JSON.stringify(asked));
      check('with your new answers to them', asked.prompt === 'My answers to the questions you just asked:\n"Which size?"="Small"\n"Which days?"="Mon, Tue, only mornings"', JSON.stringify(asked.prompt));
      await until(function () { return !dialog(); }, 3000, 'the dialog stayed');
      check('and closes the dialog', true);

      await menuOf(answers());
      menuItem('Rewind to these questions…')();
      await until(dialog, 3000, 'no dialog the second time');
      check('it asks again from your answers as they were', on('Large'), String(on('Large')));
      all('.ca-rewinddlg .ca-question-steps button')[1].click();
      await wait(50);
      choice('Mon').click();
      type(q('.ca-rewinddlg .ca-question-own'), '');
      all('.ca-rewinddlg .ca-question-steps button')[0].click();
      await wait(50);
      check('with a question left with no answer, the button is Next', primary().textContent === 'Next', primary().textContent);
      primary().click();
      await wait(50);
      check('which goes to it', /Which days\?/.test(shown()), shown());
      key(document.activeElement, { key: 'Escape' });
      await until(function () { return !dialog(); }, 3000, 'Escape left the dialog up');
      check('Escape closes it without a fork', calls('fork_session').length === 1);
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
