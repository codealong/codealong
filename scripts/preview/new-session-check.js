// Checks a new session waiting for its first prompt (newSession.ts, NewSession.tsx, App.tsx): the
// quick panel's start with nothing typed, in the background, asks the main window for one, and
// Sessions shows it in place of the selected row's panel with the cursor in its box, saying what
// sending does, with Send off while the box is empty. What's written is kept for the worktree: a
// row picked puts the panel away, and opening another there brings it back. A start that fails says
// why in the box, keeping what was written; one that goes says it's starting until the board lists
// the session, which then takes the panel's place, selected, with the cursor in its box, and the
// kept words are gone.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/new-session-check.js
(function () {
  // This runs before the app's script: pick a worktree of this Mac's and an account that starts
  // in the background.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var accounts = read('./start-options.json').accounts;
  var account = accounts.filter(function (a) { return a.places.some(function (p) { return p.place === 'background'; }); })[0];
  var lane = board.lanes.filter(function (l) { return !l.hidden && !l.device && !l.startsChats && l.sessions.some(function (s) { return !s.archived; }); })[0];
  if (!account || !lane) { stopped('no worktree with a session, or no account that starts in the background (start-options.json)'); return; }
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  var kept = 'codealong.draft.new:' + lane.id;
  localStorage.removeItem(kept);

  var PROMPT = 'Tidy the login page';
  var KEY = 'claude:' + account.name + ':0f5e1a2b-new-session-check';
  var draft = {
    spec: { laneId: lane.id, account: account.name, place: 'background', prompt: '', model: null, effort: null, worktree: false, todos: [] },
    does: 'Starts ' + account.name + ' in ' + lane.callsign + ' in the background.',
    placeLabel: 'Background',
    harness: account.harness,
  };
  // The first start fails, the second goes.
  var starts = 0;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'start_from_quick') return real(cmd, args, options);
    window.__calls.push([cmd, args]);
    starts += 1;
    return starts === 1 ? Promise.reject('That worktree just changed. Try again.') : Promise.resolve({ key: KEY, message: null });
  };

  var panel = function () { return q('.ca-newsess'); };
  var box = function () { return q('.ca-newsess .ca-composer textarea'); };
  var send = function () { return q('.ca-newsess .ca-composer-send'); };
  var note = function () { var n = q('.ca-newsess .ca-composer-note'); return n ? n.textContent : ''; };
  var title = function () { var t = q('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var write = function (text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), text);
    box().dispatchEvent(new Event('input', { bubbles: true }));
    return wait(100);
  };
  var open = function () {
    window.__emit('reveal', { kind: 'new', draft: draft });
    return until(function () { return !!box(); }, 3000, 'no new session in the panel');
  };
  var rows = function () { return all('[role="listbox"] [role="option"]'); };

  say('new session check: waiting for Sessions');
  until(function () { return rows().length > 1; }, 20000, 'Sessions lists fewer than two rows')
    .then(open)
    .then(function () {
      check('Sessions shows the new session in the panel', title() === 'New session', title());
      return until(function () { return document.activeElement === box(); }, 2000, 'the cursor isn’t in its box').catch(function (e) { check('the cursor is in its box', false, String(e)); });
    })
    .then(function () {
      check('the cursor is in its box', document.activeElement === box(), document.activeElement && document.activeElement.className);
      check('it says what sending does', panel().textContent.indexOf(draft.does + ' Nothing starts until you send its first prompt.') >= 0, panel().textContent.slice(0, 300));
      check('its box says where it runs', /Background/.test(q('.ca-newsess .ca-boxplace').textContent), q('.ca-newsess .ca-boxplace').textContent);
      check('Send is off while the box is empty', send().disabled, String(send().disabled));
      return write(PROMPT);
    })
    .then(function () {
      check('Send is on once something is written', !send().disabled, String(send().disabled));
      check('what’s written is kept for the worktree', (localStorage.getItem(kept) || '').indexOf(PROMPT) >= 0, localStorage.getItem(kept));
      // Another row picked: the session's panel shows in its place.
      var other = rows().filter(function (r) { return r.getAttribute('aria-selected') !== 'true'; })[0];
      other.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return until(function () { return !panel(); }, 2000, 'picking a row left the new session up');
    })
    .then(function () {
      check('a row picked puts it away', !panel() && title() !== 'New session', title());
      return open();
    })
    .then(function () {
      check('opening another there brings back what was written', box().value === PROMPT, box().value);
      key(box(), { key: 'Enter' });
      return until(function () { return /worktree just changed/.test(note()); }, 3000, 'a failed start said nothing');
    })
    .then(function () {
      check('a failed start says why in the box', /worktree just changed/.test(note()), note());
      check('keeping what was written', box().value === PROMPT && !!panel(), box().value);
      send().click();
      return until(function () { return /Starting/.test(note()); }, 3000, 'no Starting… once the start went');
    })
    .then(function () {
      var spec = calls('start_from_quick').pop()[1].spec;
      check('the send starts it as the quick panel would have, on what was written', spec.laneId === lane.id && spec.account === account.name && spec.place === 'background' && spec.prompt === PROMPT, JSON.stringify(spec));
      check('it says it’s starting until the board lists it', !!panel() && box().readOnly, note());
      check('what was written is no longer kept', !localStorage.getItem(kept), localStorage.getItem(kept));
      // The board lists the session it started.
      var b = JSON.parse(JSON.stringify(board));
      var home = b.lanes.filter(function (l) { return l.id === lane.id; })[0];
      var made = JSON.parse(JSON.stringify(home.sessions[0]));
      made.key = KEY;
      made.id = '0f5e1a2b-new-session-check';
      made.account = account.name;
      made.title = PROMPT;
      made.status = 'working';
      made.archived = false;
      made.live = true;
      made.lastActivity = made.lastPromptAt = made.startedAt = Date.now();
      home.sessions.unshift(made);
      window.__emit('board', b);
      return until(function () { return title() === PROMPT; }, 5000, 'the session it started didn’t take the panel’s place');
    })
    .then(function () {
      var row = document.getElementById('row-' + lane.id + '::' + KEY);
      check('the session it started takes the panel’s place', !panel() && title() === PROMPT, title());
      check('selected in Sessions', !!row && row.getAttribute('aria-selected') === 'true', row ? row.outerHTML.slice(0, 160) : 'no row');
      return until(function () { var a = document.activeElement; return !!a && !!a.closest('.ca-sessinsp .ca-composer'); }, 3000, 'the cursor isn’t in its box').then(
        function () { check('with the cursor in its box', true); },
        function (e) { check('with the cursor in its box', false, String(e)); },
      );
    })
    .then(done, stopped);
})();
