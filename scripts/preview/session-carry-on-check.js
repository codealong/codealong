// Checks Carry on for several sessions picked together (menus.ts `sessionsActions`,
// SessionPickPanel.tsx), as a restart leaves many cut off: four Claude Code sessions of this
// device's, two cut off and two idle, none running. Picked together, the panel leads with Carry on
// for the two cut off, a right-click's menu has it too, and a click resumes each in turn, in the
// list's order, with one toast for the lot. Without one cut off among them there's no Carry on.
//   scripts/preview.sh sessions '' scripts/preview/session-carry-on-check.js
(function () {
  // This runs before the app's script: the board it gets has four sessions, made from real ones
  // on this device, and no other.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  localStorage.removeItem('codealong.sessionSort');
  localStorage.removeItem('codealong.age');
  var now = Date.now();
  var plan = [
    { status: 'error', at: now - 5 * 60e3, cut: true },
    { status: 'error', at: now - 6 * 60e3, cut: true },
    { status: 'idle', at: now - 60 * 60e3 },
    { status: 'idle', at: now - 120 * 60e3 },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    // A resume in a terminal, as the engine answers it.
    if (cmd === 'resume_session') return p.then(function () { return { message: 'Resumed.', opened: false }; });
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.pinned = [];
      board.lanes.forEach(function (l) {
        l.sessions = l.sessions.filter(function (s) {
          if (picked.some(function (x) { return x.key === s.key && x.lane === l.id; })) return true;
          // Claude Code's in a terminal, whose accounts resume in one.
          if (l.hidden || l.device || s.visitor || s.archived || s.helper || s.headless || s.harness !== 'claude' || !s.resumeCommand || seen[s.key] || picked.length >= plan.length) return false;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.at;
          s.lastActivity = p.at;
          s.lastPromptAt = p.at;
          s.cutOff = p.cut ? p.at : undefined;
          s.limit = undefined;
          s.connectionLost = undefined;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.live = false;
          s.surface = 'cli';
          s.pid = null;
          s.snoozedUntil = undefined;
          s.title = 'Pick ' + 'ABCD'[picked.length];
          s.ownWorktree = undefined;
          s.background = undefined;
          s.attachCommand = null;
          picked.push({ key: s.key, lane: l.id });
          return true;
        });
      });
      return board;
    });
  };

  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  // The keys as the list has them, top to bottom.
  var listOrder = function () {
    return all('.ca-sessionlist [role=option]').map(function (el) {
      var x = picked.filter(function (x) { return el.id === 'row-' + x.lane + '::' + x.key; })[0];
      return x ? x.key : null;
    }).filter(Boolean);
  };
  var selectedRows = function () {
    return picked.map(function (_, i) { return row(i) && row(i).getAttribute('aria-selected') === 'true' ? 'ABCD'[i] : ''; }).join('');
  };
  var press = function (i, init) { row(i).dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var pickPanel = function () { return q('.ca-pick'); };
  var head = function () { var h = q('.ca-pick-head'); return h ? h.textContent : ''; };
  var hotkeys = function () { return all('.ca-pick .ca-hotkey').map(function (b) { return b.textContent; }); };
  var hotkey = function (label) {
    return all('.ca-pick .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };

  say('session carry-on check: waiting for the list');
  until(function () { return picked.length === plan.length && row(3); }, 20000, 'four Claude Code sessions listed')
    .then(function () { return wait(300); })
    .then(function () {
      press(0);
      return until(function () { return title() === 'Pick A'; }, 3000, 'Pick A’s panel');
    })
    .then(function () {
      press(1, { metaKey: true });
      return wait(100);
    })
    .then(function () {
      press(2, { metaKey: true });
      return wait(100);
    })
    .then(function () {
      press(3, { metaKey: true });
      return until(function () { return head() === '4 sessions'; }, 3000, 'the pick panel with all four').catch(function (e) {
        throw new Error(e.message + ' (picked ' + selectedRows() + ', ' + head() + ')');
      });
    })
    .then(function () {
      check('⌘-click picks all four', selectedRows() === 'ABCD', selectedRows());
      // Start options are read once a window needs them: the panel has Carry on once they are.
      return until(function () { return hotkey('Carry on'); }, 5000, 'Carry on on the panel (does a Claude Code account here resume in a terminal?)');
    })
    .then(function () {
      var keys = hotkeys();
      check('the panel leads with Carry on, saying it takes the two cut off', keys[0] === 'Carry on 2', keys.join(' | '));
      return menuOf(row(2));
    })
    .then(function (labels) {
      check('a right-click on a picked row has it first', labels[0] === 'Carry on 2 sessions', labels.join(' | '));
      check('and leaves the pick', selectedRows() === 'ABCD', selectedRows());
      hotkey('Carry on').click();
      return until(function () { return sent('resume_session').length >= 2 && toast(); }, 3000, 'both resumed');
    })
    .then(function () {
      var asked = sent('resume_session');
      var cut = listOrder().filter(function (k) { return k === picked[0].key || k === picked[1].key; });
      check('a click resumes the two cut off, one after another in the list’s order', cut.length === 2 && JSON.stringify(asked.map(function (a) { return a.key; })) === JSON.stringify(cut), JSON.stringify(asked) + ' for ' + cut.join(', '));
      check('each with nothing to say, so it’s told to carry on', asked.every(function (a) { return a.prompt === '' && a.resume && a.resume.place && a.resume.model === null; }), JSON.stringify(asked));
      check('one toast says so', /Resumed 2 sessions to carry on\./.test(toast()), toast());
      check('the pick stays', !!pickPanel() && head() === '4 sessions', head());
      keyDown({ key: 'Escape', code: 'Escape' });
      return wait(100);
    })
    .then(function () {
      press(2);
      return until(function () { return title() === 'Pick C'; }, 3000, 'Pick C’s panel');
    })
    .then(function () {
      press(3, { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel again');
    })
    .then(function () {
      check('with none cut off there’s no Carry on', !hotkey('Carry on') && hotkeys()[0].indexOf('Seen') === 0, hotkeys().join(' | '));
      return menuOf(row(3));
    })
    .then(function (labels) {
      check('nor in their menu', labels.every(function (l) { return l.indexOf('Carry on') < 0; }), labels.join(' | '));
    })
    .then(done)
    .catch(stopped);
})();
