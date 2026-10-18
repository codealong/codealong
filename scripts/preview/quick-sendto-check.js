// Checks Send to in the quick panel (Quick.tsx): under a prompt, a row under Jev's suggestions
// (and ⌘F) lists every session the prompt could go to, Jev's pick first with how likely it is,
// while the prompt waits under the box, as written, and the new session's row and bar are put away. What's
// typed in the box then finds a session; ⏎ sends the prompt to it and puts the panel away; ⌫ in
// the empty box or Escape bring the prompt back to the box; and with nothing found, ⏎ neither
// sends nor starts anything.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-sendto-check.js
// The board needs two sessions in worktrees that aren't hidden.
(function () {
  // This runs before the app's script: two of the board's sessions get titles nothing else has,
  // both running and waiting for a prompt, one Jev's pick and the other the one you last
  // prompted. Jev is on, and the text search finds nothing, so what lists them is Send to alone.
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.quick.archived');
  var PICKED = 'Feed the axolotl';
  var OTHER = 'Groom the okapi';
  var PROMPT = 'Please tidy the release notes before Friday';
  var keys = {};
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    if (cmd === 'route_quick') {
      return Promise.resolve({
        action: 'send',
        actions: [['send', 0.9], ['start', 0.1]],
        sessions: keys.picked ? [{ id: keys.picked, p: 0.8 }] : [],
        noSession: 0.1,
        lanes: [],
        repos: [],
        ownWorktree: 0,
        ownWorktreeAsked: 0,
        habit: { new: 0, of: 0 },
        duplicate: null,
        model: 'jev-1.14.0',
        ms: 200,
        inputTokens: 9000,
      });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.jev = Object.assign({}, board.jev, { quick_panel: true });
      board.todos = [];
      board.prs = [];
      var n = 0;
      board.lanes.forEach(function (l) {
        if (l.hidden || l.device) return;
        l.sessions.forEach(function (s) {
          if (s.visitor || s.archived || s.helper || s.headless) return;
          if (n < 2) {
            var name = n === 0 ? 'picked' : 'other';
            if (keys[name] && keys[name] !== s.key) return;
            keys[name] = s.key;
            s.title = n === 0 ? PICKED : OTHER;
            s.status = 'idle';
            s.live = true;
            delete s.snoozedUntil;
            // The other is the one you prompted last, so it leads what Jev didn't pick.
            if (n === 1) s.lastPromptAt = Date.now();
          }
          n++;
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-scroll .ca-quick-row')); };
  var row = function (text) { return rows().filter(function (r) { return r.textContent.indexOf(text) >= 0; })[0]; };
  var entry = function () { return row('Send to another session'); };
  var strip = function () { return document.querySelector('.ca-quick-sending'); };
  var heading = function () { var h = document.querySelector('.ca-quick-glabel'); return h ? h.textContent : ''; };
  var compose = function () { return document.querySelector('.ca-quick-compose'); };
  var bar = function () { return document.querySelector('.ca-quick-bar'); };
  var cmdF = function () { key(input(), { key: 'f', code: 'KeyF', metaKey: true }); };
  var prompt = function () {
    type(input(), '');
    type(input(), PROMPT);
    return until(function () { return !!entry() && !!row('Send to “' + PICKED); }, 5000, 'Jev’s suggestion and the row for Send to');
  };

  say('quick send to check: waiting for the list');
  until(function () { return !!keys.other && !!input() && rows().length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(prompt)
    .then(function () {
      var e = entry();
      check('the row is under Jev’s suggestions', /^Suggested/i.test(e.closest('.ca-quick-group').textContent), e.closest('.ca-quick-group').textContent);
      check('it says how many sessions there are and its key', /\d+ sessions/.test(e.textContent) && !!e.querySelector('.ca-kbds') && /⌘\s*F/.test(e.querySelector('.ca-kbds').textContent), e.textContent);
      cmdF();
      return until(function () { return !!strip() && /^Send to/i.test(heading()); }, 3000, 'Send to');
    })
    .then(function () {
      check('the box is empty for finding a session', input().value === '' && /send this to/.test(input().placeholder), input().value);
      check('the prompt waits under the box, as written', strip().textContent.indexOf(PROMPT) >= 0 && strip().getBoundingClientRect().top >= input().getBoundingClientRect().bottom, strip().textContent);
      check('the new session’s row and bar are put away', !compose() && !bar());
      var first = rows()[0];
      check('Jev’s pick leads, with how likely it is', !!first && first.textContent.indexOf(PICKED) >= 0 && /80%/.test(first.textContent), first && first.textContent);
      check('the one you prompted last comes next', !!rows()[1] && rows()[1].textContent.indexOf(OTHER) >= 0, rows()[1] && rows()[1].textContent);
      check('the selected row says ⏎ sends', /⏎ Send/.test(first.textContent) && /⏎ sends it/.test(first.title), first.title);
      type(input(), 'okapi');
      return until(function () { return rows().length === 1 && !!row(OTHER); }, 3000, 'the box finding the other session');
    })
    .then(function () {
      check('what’s typed in the box finds a session, by its title', row(OTHER).classList.contains('is-active'));
      key(window, { key: 'Escape' });
      return until(function () { return !strip(); }, 3000, 'the prompt back in the box');
    })
    .then(function () {
      check('Escape brings the prompt back to the box', input().value === PROMPT, input().value);
      check('and the new session’s row and bar', !!compose() && !!bar());
      cmdF();
      return until(function () { return !!strip(); }, 3000, 'Send to again');
    })
    .then(function () {
      key(input(), { key: 'Backspace' });
      return until(function () { return !strip(); }, 3000, 'the prompt back after ⌫');
    })
    .then(function () {
      check('⌫ in the empty box brings the prompt back too', input().value === PROMPT, input().value);
      cmdF();
      return until(function () { return !!strip(); }, 3000, 'Send to once more');
    })
    .then(function () {
      type(input(), 'nothing has this zzqx');
      return until(function () { return rows().length === 0; }, 3000, 'nothing found');
    })
    .then(function () {
      var note = document.querySelector('.ca-quick-note');
      check('with nothing found it says so', !!note && /No session matches/.test(note.textContent), note && note.textContent);
      var before = { sends: sent('send_to_session').length, starts: sent('start_from_quick').length };
      key(input(), { key: 'Enter' });
      key(input(), { key: 'Enter', metaKey: true });
      return wait(300).then(function () {
        check('⏎ then neither sends nor starts', sent('send_to_session').length === before.sends && sent('start_from_quick').length === before.starts);
        type(input(), 'okapi');
        return until(function () { return !!row(OTHER) && row(OTHER).classList.contains('is-active'); }, 3000, 'the other session found');
      });
    })
    .then(function () {
      var hides = sent('hide_quick').length;
      key(input(), { key: 'Enter' });
      return until(function () { return sent('send_to_session').length > 0; }, 3000, 'the send').then(function () {
        var s = sent('send_to_session').pop();
        check('⏎ sends the prompt to it', s.key === keys.other && s.text === PROMPT, JSON.stringify(s));
        check('and puts the panel away', sent('hide_quick').length === hides + 1);
        check('the panel is fresh after', input().value === '' && !strip());
      });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
