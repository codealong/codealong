// Checks the quick panel set to listen (Quick.tsx, Listen now), as `[shortcuts] listen` opens it:
// the box asks what the meeting is and the hint names that shortcut; nothing is listed or looked
// for, whatever is typed; it's a chat of its own, with only the worktree to pick; the meeting the
// calendar has going on is the one it's of until you say it isn't; ⏎ asks for the listening with
// what's typed, what's picked and where, and puts the panel away; how it was asked is kept for
// the next one; a start that fails brings the panel back with what was typed and why, set to
// listen again even when it has been opened for something else since, a prompt written there
// meanwhile kept as a draft; with nothing picked to hear it says so and asks for nothing; and an
// ordinary open after lists and starts as before, with no draft made of a meeting's name.
//   PREVIEW_VOICE=1 PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-listen-check.js
// The board needs a worktree to pick.
(function () {
  ['codealong.quick.listen', 'codealong.quick.draft', 'codealong.quick.history'].forEach(function (k) { localStorage.removeItem(k); });
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘T\)/.test(b.title); })[0]; };
  var row = function () { return q('.ca-quick-compose'); };
  var title = function () { return (q('.ca-quick-compose .ca-quick-row-title') || {}).textContent; };
  var why = function () { return (q('.ca-quick-compose .ca-quick-row-why') || {}).textContent || ''; };
  var checks = function () { return all('.ca-quick-listen .ca-listendlg-check'); };
  var box = function (words) { return checks().filter(function (l) { return l.textContent.indexOf(words) >= 0; })[0]; };
  var role = function (name) { return all('.ca-quick-listen .ca-segctl button').filter(function (b) { return b.textContent === name; })[0]; };
  var enter = function () { key(input(), { key: 'Enter' }); };
  var board;
  var lane;
  var open = function () {
    window.__quickOpen({ listen: true }, board);
    return until(function () { return box('Design review'); }, 5000, 'the calendar’s meeting');
  };

  say('quick listen check: waiting for the panel');
  until(function () { return window.__quickOpen && input(); }, 20000, 'the quick panel’s page')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (b) {
      board = Object.assign({}, b, { listenShortcut: 'Option+Cmd+=' });
      lane = board.lanes.filter(function (l) { return !l.startsChats && !l.device; })[0];
      if (!lane) throw new Error('the board has no worktree to pick');
      return open();
    })
    .then(function () {
      check('the box asks what the meeting is', /^What’s this meeting\?/.test(input().placeholder), input().placeholder);
      var hint = q('.ca-quick-hotkey');
      check('the hint names the listen shortcut', !!hint && hint.textContent === '⌥⌘=', hint && hint.textContent);
      check('nothing is listed', all('[data-row]').length === 0 && all('.ca-quick-glabel').length === 0, all('[data-row]').length + ' rows');
      check('it’s a chat of its own', /No project/.test(chip().textContent), chip().textContent);
      check('with only the worktree to pick', !q('.ca-quick-newtree') && q('.ca-quick-agent').hidden, String(!!q('.ca-quick-newtree')));
      check('the card is its own height', Math.round(q('.ca-quick').getBoundingClientRect().height) === 500, String(q('.ca-quick').getBoundingClientRect().height));
      check('it’s of the meeting going on', box('Design review').querySelector('input').checked && title() === 'Listen to “Design review”', title());
      check('and says what ⏎ does', /hears you and this Mac’s audio from the moment you press ⏎, and a session started as a chat of its own keeps its notes/.test(why()), why());
      type(input(), 'On-site with Kestrel');
      return wait(900);
    })
    .then(function () {
      check('what’s typed is looked for nowhere', sent('search_text').length === 0 && sent('route_quick').length === 0 && all('[data-row]').length === 0, sent('search_text').length + ' searches, ' + sent('route_quick').length + ' routes');
      role('Coach').click();
      box('Design review').querySelector('input').click();
      return until(function () { return title() === 'Listen now'; }, 5000, 'the meeting unticked');
    })
    .then(function () {
      check('a coach is said on the row', /coaches you through it/.test(why()), why());
      check('the keyboard stays in the box', document.activeElement === input(), document.activeElement && document.activeElement.tagName);
      enter();
      return until(function () { return sent('listen_now').length === 1; }, 5000, 'the listening asked for');
    })
    .then(function () {
      var asked = last('listen_now');
      check('⏎ asks for a meeting of its own', asked.meeting === null && asked.lane === null, JSON.stringify([asked.meeting, asked.lane]));
      check('as a coach, hearing both, with what was typed', asked.ask.role === 'coach' && asked.ask.mic && asked.ask.system && asked.ask.prompt === 'On-site with Kestrel', JSON.stringify(asked.ask));
      check('and puts the panel away, empty', calls('hide_quick').length > 0 && input().value === '', input().value);
      return open();
    })
    .then(function () {
      check('how it was asked is kept', role('Coach').getAttribute('aria-checked') === 'true', localStorage.getItem('codealong.quick.listen'));
      check('and the meeting going on is offered again', box('Design review').querySelector('input').checked);
      enter();
      return until(function () { return sent('listen_now').length === 2; }, 5000, 'the calendar’s meeting asked for');
    })
    .then(function () {
      check('with it ticked, ⏎ takes that meeting', last('listen_now').meeting === 'E-now@1', String(last('listen_now').meeting));
      return open();
    })
    .then(function () {
      // A start that fails: the panel comes back with what was typed and why.
      window.__LISTEN_FAILS__ = 'macOS isn’t letting Codealong use the microphone.';
      type(input(), 'Standup');
      enter();
      return until(function () { return q('.ca-quick-error'); }, 5000, 'why it failed');
    })
    .then(function () {
      window.__LISTEN_FAILS__ = null;
      check('a start that fails says why', /isn’t letting Codealong use the microphone/.test(q('.ca-quick-error').textContent), q('.ca-quick-error').textContent);
      check('with what was typed', input().value === 'Standup' && last('show_quick').context.keep === true, input().value);
      // Nothing picked to hear: said on the row, and ⏎ asks for nothing.
      box('through the microphone').querySelector('input').click();
      box('from what this Mac plays').querySelector('input').click();
      return until(function () { return /Pick something to listen to/.test(why()); }, 5000, 'nothing to hear');
    })
    .then(function () {
      var before = sent('listen_now').length;
      check('with nothing to hear the row says so', row().classList.contains('is-disabled'), row().className);
      enter();
      return wait(300).then(function () {
        check('and ⏎ asks for nothing', sent('listen_now').length === before, String(sent('listen_now').length));
        box('through the microphone').querySelector('input').click();
        chip().click();
        return until(function () { return all('.ca-picker-opt').length > 0; }, 5000, 'the worktree picker');
      });
    })
    .then(function () {
      var o = all('.ca-picker-opt').filter(function (x) { return x.textContent.indexOf(lane.callsign) >= 0; })[0];
      if (!o) throw new Error('the picker doesn’t list ' + lane.callsign);
      o.click();
      return until(function () { return chip().textContent.indexOf(lane.callsign) >= 0; }, 5000, 'the worktree picked');
    })
    .then(function () {
      check('a worktree picked is where its session starts', why().indexOf('a session started in ' + lane.callsign) >= 0, why());
      var before = sent('listen_now').length;
      enter();
      return until(function () { return sent('listen_now').length > before; }, 5000, 'the listening asked for in a worktree');
    })
    .then(function () {
      var asked = last('listen_now');
      check('⏎ asks for it there, hearing only you', asked.lane === lane.id && asked.ask.mic && !asked.ask.system, JSON.stringify([asked.lane, asked.ask.mic, asked.ask.system]));
      return open();
    })
    .then(function () {
      // A start that fails once the panel has been opened for something else and written in
      // (its session can take many seconds not to come): it's back set to listen, not finding
      // sessions, and what was written there meanwhile is kept as a draft.
      window.__LISTEN_FAILS__ = 'Its session was started but hasn’t shown yet.';
      window.__LISTEN_AFTER__ = 700;
      type(input(), 'Retro');
      enter();
      window.__quickOpen({}, board);
      type(input(), 'Fix the login redirect on the settings page');
      return until(function () { return q('.ca-quick-error'); }, 5000, 'why it failed, later');
    })
    .then(function () {
      window.__LISTEN_FAILS__ = null;
      window.__LISTEN_AFTER__ = 0;
      check('a start that fails later comes back set to listen', !!q('.ca-quick-listen') && input().value === 'Retro' && all('[data-row]').length === 0 && /^What’s this meeting\?/.test(input().placeholder), input().value + ' | ' + input().placeholder);
      return wait(900);
    })
    .then(function () {
      var looked = sent('search_text').map(function (a) { return a.words; }).concat(sent('route_quick').map(function (a) { return a.text; }));
      check('its name is looked for nowhere', looked.every(function (w) { return !/Retro|Kestrel|Standup/.test(w); }), looked.join(' | '));
      var draft = JSON.parse(localStorage.getItem('codealong.quick.draft') || 'null');
      check('and the prompt written meanwhile is kept as a draft', !!draft && draft.text === 'Fix the login redirect on the settings page', JSON.stringify(draft));
      type(input(), 'Planning');
      return wait(300);
    })
    .then(function () {
      window.__quickOpen({}, board);
      return until(function () { return q('[data-row="compose"]'); }, 5000, 'an ordinary open');
    })
    .then(function () {
      check('an ordinary open after lists and starts as before', !q('.ca-quick-listen') && Math.round(q('.ca-quick').getBoundingClientRect().height) === 640 && !!q('.ca-quick-newtree'), String(q('.ca-quick').getBoundingClientRect().height));
      var draft = JSON.parse(localStorage.getItem('codealong.quick.draft') || 'null');
      check('and a meeting’s name is no draft to bring back', !!draft && draft.text === 'Fix the login redirect on the settings page' && /brings back your draft/.test(input().placeholder), JSON.stringify(draft));
      done();
    })
    .catch(stopped);
})();
