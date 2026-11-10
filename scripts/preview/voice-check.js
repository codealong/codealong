// Checks voice in the main window (src/listening.ts, ui/Listening.tsx, talk.ts). Off, as a new
// config.toml has it, a session's menu has no Listen…. On, it has: its dialog asks what the
// session does with what it hears and what it hears, won't start with nothing to hear, says why
// a start failed and stays, and asks the engine as picked. A session that listens wears a chip
// on its row, has Stop listening in its menu, and shows the meeting in its conversation (what was
// heard as lines, its agent's answers between them with a cue's lines kept apart, an answer that
// says nothing not at all) and a strip over its box with what it hasn't been told yet, the words
// being said, its notes and Stop. And what's said with a key held to talk is typed where the
// caret is, or else into the box of the session shown, or of the one the engine names.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/voice-check.js
(function () {
  // This runs before the app's script: the board it gets has two sessions made from real ones
  // on this device, with voice and the listening as the check has them at the time.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  window.__PREVIEW_VOICE__ = true;
  var now = Date.now();
  var plan = [{ title: 'Voice A' }, { title: 'Voice B' }];
  var picked = [];
  var voice = false;
  var listens = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      var fresh = picked.length === 0;
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (fresh && (l.hidden || l.device || s.visitor || s.archived || s.helper || seen[s.key] || picked.length >= plan.length)) return;
          var at = fresh ? picked.length : picked.map(function (p) { return p.key; }).indexOf(s.key);
          if (at < 0 || seen[s.key]) return;
          seen[s.key] = true;
          s.title = plan[at].title;
          s.status = 'idle';
          s.pending = undefined;
          s.asks = false;
          s.lastActivity = now - (at + 1) * 60e3;
          s.lastPromptAt = s.lastActivity;
          if (fresh) picked.push({ key: s.key, lane: l.id });
        });
      });
      board.experimental = Object.assign({}, board.experimental, { voice: voice });
      board.listenings = undefined;
      if (listens) {
        board.listenings = {};
        board.listenings[picked[0].key] = { id: 'sample', startedAt: Date.now() - 247e3, role: 'coach', mic: true, playback: true, transcript: '/t/transcript.md', notes: '/t/notes.md', cueAt: 0 };
      }
      return board;
    });
  };
  var again = function () { return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); return wait(200); }); };

  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var show = function (i) {
    row(i).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === plan[i].title; }, 3000, 'its panel').then(function () { return wait(100); });
  };
  var dialog = function () { return q('.ca-listendlg'); };
  var button = function (root, text) { return all('button', root).filter(function (b) { return b.textContent.trim() === text; })[0]; };
  var box = function (text) { return all('.ca-listendlg-check', dialog()).filter(function (l) { return l.textContent.indexOf(text) === 0; })[0].querySelector('input'); };
  var start = function () { return button(dialog(), 'Start listening'); };
  var aside = function () { var a = q('.ca-listendlg-aside', dialog()); return a ? a.textContent.trim() : ''; };
  var strip = function () { return q('.ca-sessinsp .ca-live'); };
  var stripLines = function () { return all('.ca-live-lines .ca-heard-line:not(.is-live)', strip()).map(function (l) { return l.querySelector('.ca-heard-text').textContent; }); };
  var live = function () { var l = q('.ca-live-lines .ca-heard-line.is-live', strip()); return l ? l.querySelector('.ca-heard-who').textContent + ': ' + l.querySelector('.ca-heard-text').textContent : ''; };
  var hearing = function (h) { window.__emit('hearing', Object.assign({ session: picked[0].key }, h)); return wait(80); };
  var field = function () { return q('.ca-sessinsp .ca-composer textarea'); };
  var draft = function (key) { try { return JSON.parse(localStorage.getItem('codealong.draft.' + key)).text; } catch (e) { return ''; } };

  say('voice check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && row(1); }, 20000, 'two sessions of this Mac to stand in')
    .then(function () { return show(1); })
    .then(function () { return menuOf(row(0)); })
    .then(function (labels) {
      check('off, a session’s menu has no Listen…', labels.indexOf('Listen…') < 0 && labels.indexOf('Stop listening') < 0, labels.join(' | '));
      voice = true;
      return again();
    })
    .then(function () { return menuOf(row(0)); })
    .then(function (labels) {
      check('on, it has', labels.indexOf('Listen…') >= 0, labels.join(' | '));
      menuItem('Listen…')();
      return until(dialog, 3000, 'the dialog');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('its dialog names the session', q('.ca-dialog-lede', dialog()).textContent.indexOf('Voice A hears a meeting') === 0, q('.ca-dialog-lede', dialog()).textContent);
      check('and offers to listen to one app that plays', !!button(dialog(), 'Only Meet'));
      button(dialog(), 'Coach').click();
      box('You, through').click();
      box('Everyone else').click();
      return wait(150);
    })
    .then(function () {
      check('with nothing to hear it won’t start, and says so', start().disabled && aside().indexOf('Pick something to listen to') === 0, aside());
      box('You, through').click();
      box('Everyone else').click();
      type(q('.ca-listendlg-prompt', dialog()), 'I want a signed order.');
      return wait(150);
    })
    .then(function () {
      check('with something to hear it will', !start().disabled && aside() === '', aside());
      window.__LISTEN_FAILS__ = 'Codealong isn’t allowed to use the microphone. Allow it in System Settings › Privacy & Security › Microphone.';
      start().click();
      return until(function () { return aside().indexOf('isn’t allowed') >= 0; }, 3000, 'why it didn’t start');
    })
    .then(function () {
      check('a start that failed says why, and the dialog stays', !!dialog() && !start().disabled);
      window.__LISTEN_FAILS__ = null;
      start().click();
      return until(function () { return !dialog(); }, 3000, 'the dialog to close');
    })
    .then(function () {
      var asked = last('listen');
      check('it asks the engine as picked', asked.key === picked[0].key && asked.on === true && asked.ask.role === 'coach' && asked.ask.mic && asked.ask.system && asked.ask.apps.length === 0 && asked.ask.prompt === 'I want a signed order.' && !asked.ask.start, JSON.stringify(asked));
      // The board says it listens.
      window.__LISTENS__ = picked[0].key;
      listens = true;
      return again();
    })
    .then(function () {
      check('a session that listens wears a chip on its row', !!q('.ca-listeningchip', row(0)) && !q('.ca-listeningchip', row(1)));
      return menuOf(row(0));
    })
    .then(function (labels) {
      check('and has Stop listening in its menu, in place of Listen…', labels.indexOf('Stop listening') >= 0 && labels.indexOf('Listen…') < 0, labels.join(' | '));
      return show(0);
    })
    .then(function () { return until(function () { return strip() && stripLines().length === 2 && all('.ca-thread .ca-heard-line').length > 0; }, 5000, 'its strip and the meeting'); })
    .then(function () {
      var thread = q('.ca-thread');
      var said = all('.ca-heard-lines .ca-heard-line', thread).map(function (l) { return l.querySelector('.ca-heard-who').textContent; });
      check('its conversation shows what was heard, as the meeting’s lines', said.join(',') === 'You,Them,Them,You,Them', said.join(','));
      check('with Codealong’s own line where its hearing broke', all('.ca-heard-aside', thread).length === 1 && all('.ca-heard-aside', thread)[0].textContent.indexOf('stopped and started again') > 0);
      check('and that it began, folded', !!q('.ca-heard-toggle', thread) && !q('.ca-heard-brief', thread));
      var agents = all('.ca-thread-said.is-agent', thread);
      check('an answer that says nothing isn’t drawn', agents.length === 3 && agents.every(function (a) { return a.textContent.trim().slice(-1) !== '–'; }), String(agents.length));
      var cue = agents[2];
      var lines = all('.ca-md p', cue).map(function (p) { return p.textContent; });
      check('a cue’s lines stay lines', lines.length === 2 && lines[1] === 'They haven’t named a budget owner yet.', lines.join(' / '));
      check('the strip says it’s listening, for how long and to what', /^Listening\s*4:\d\d · You and this Mac’s audio/.test(q('.ca-live-head', strip()).textContent), q('.ca-live-head', strip()).textContent);
      check('with what it hasn’t been told yet', stripLines().join(' / ') === 'Could Dana join us Thursday? / I’ll ask her today.', stripLines().join(' / '));
      // Its clock goes on by the second, whatever the panel's own does.
      var clockSays = function () { return (/\d+:\d\d/.exec(q('.ca-live-meta', strip()).textContent) || [''])[0]; };
      var was = clockSays();
      return until(function () { return clockSays() !== was; }, 2500, 'the strip’s clock to tick within two seconds').then(function () {
        check('how long it’s listened goes on by the second', true);
        return hearing({ who: 'Them', text: 'and one more' });
      });
    })
    .then(function () {
      check('and the words being said', live() === 'Them: and one more', live());
      window.__emit('hearing', { session: picked[1].key, who: 'Them', text: 'another meeting' });
      return wait(80);
    })
    .then(function () {
      check('another session’s aren’t its own', live() === 'Them: and one more', live());
      return hearing({ line: 9, said: '[14:05:00] Them: And one more thing.' });
    })
    .then(function () {
      check('settled, they’re a line', live() === '' && stripLines().length === 3 && stripLines()[2] === 'And one more thing.', stripLines().join(' / '));
      return hearing({ told: 9 });
    })
    .then(function () {
      check('told, they leave the strip for the conversation', stripLines().length === 0 && !!strip(), stripLines().join(' / '));
      button(strip(), 'Notes').click();
      return until(function () { return q('.ca-live-notes', strip()); }, 2000, 'its notes');
    })
    .then(function (notes) {
      check('Notes shows the notes it keeps', notes.textContent.indexOf('Northwind renewal') >= 0 && !!notes.querySelector('li'), notes.textContent.slice(0, 80));
      // A key held to talk: at the caret, else the box of the session shown, else the one named.
      var f = field();
      f.focus();
      type(f, 'Hello');
      f.setSelectionRange(5, 5);
      window.__emit('talked', { text: ' rebase onto main. ' });
      return until(function () { return field().value === 'Hello rebase onto main.'; }, 2000, 'the words at the caret');
    })
    .then(function () {
      check('what’s said is typed at the caret, a space before it', field().value === 'Hello rebase onto main.', field().value);
      field().blur();
      window.__emit('talked', { text: 'Then run the tests.' });
      return until(function () { return field().value === 'Hello rebase onto main. Then run the tests.'; }, 2000, 'the words in the shown session’s box');
    })
    .then(function () {
      check('with no caret, it goes into the box of the session shown', draft(picked[0].key) === 'Hello rebase onto main. Then run the tests.', draft(picked[0].key));
      window.__emit('talked', { text: 'What waits on me?', key: picked[1].key });
      return until(function () { return draft(picked[1].key) === 'What waits on me?'; }, 2000, 'the words in the named session’s box');
    })
    .then(function () {
      check('or of the session the engine names', field().value === 'Hello rebase onto main. Then run the tests.', field().value);
      check('and nothing is sent', sent('send_to_session').length === 0 && sent('resume_session').length === 0);
      button(strip(), 'Stop').click();
      return until(function () { var l = last('listen'); return l && l.on === false; }, 2000, 'the engine asked to stop');
    })
    .then(function () {
      check('Stop ends it', last('listen').key === picked[0].key, JSON.stringify(last('listen')));
      listens = false;
      window.__LISTENS__ = null;
      return again();
    })
    .then(function () {
      check('ended, the chip and the strip go, and the meeting stays in its conversation', !q('.ca-listeningchip', row(0)) && !strip() && all('.ca-thread .ca-heard-line').length === 5);
    })
    .then(done)
    .catch(stopped);
})();
