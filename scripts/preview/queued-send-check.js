// Checks that a message the session's harness holds for its turn shows in its conversation
// as queued for as long as the board says so, not as one that may not have reached it, and as sent
// once it's out of the queue (ahead.ts `settled`, SessionThread.tsx `Unsaid`): a session runs, a
// message goes to it, and boards come that show it held (`queuedAt`), then taken. Then the board
// lists what's held (`queued`, the Codex app's queue, Claude Code's): a message sent from here, by the id its send
// said it was queued under, shows once; one sent into the turn stays sent while another waits
// after it; one queued elsewhere shows too, after the turn; and they go as the queue empties. While
// the turn can take what's held sooner (`steerable`), each held message has Send now, which asks
// for it by its id and shows it once, as sent, in its place; one that doesn't go stays held, saying
// why. Beside it, Remove takes one out: gone from the thread at once and asked for by its id, the
// others' buttons waiting until it's done, back with why when it's still held, and one of yours that went back in after it under a new id (Claude
// Code gives back everything it holds) still showing once, as held. Between them, Edit takes one
// out the same way and its words come to the box, after what's written there and with the cursor,
// once it's out, whatever's written has in it already: yours as you wrote them, not as listed; none
// where it's still held, saying why; Put in the box where it wasn't held after all; no Edit where
// it only names a paste, or while a side conversation has the box. It ends with two messages
// queued, for the screenshot.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/queued-send-check.js
//   PREVIEW_SCHEME=dark PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/queued-send-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var boardNow = null;
  // What a held message says: Claude Code's turn takes it in once the step it's on ends.
  var QUEUED = 'Queued until its current step ends';
  // Whether a send is queued, with the id it went under (`Sent.queued`), as Codex's is.
  var queueing = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  var running = function (s) {
    s.status = 'working';
    s.statusSince = Date.now() - 10 * 60e3;
    s.live = true;
    s.hosted = false;
    s.asks = false;
    s.suggestions = undefined;
    s.title = 'Queued A';
    return s;
  };
  // Why Send now on a held message fails, or null for it to go.
  var steerFails = null;
  // How the Remove under way ends: `res` with what the engine answers, `rej` with why.
  var unqueued = null;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_unqueue') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (res, rej) { unqueued = { res: res, rej: rej }; });
    }
    if (cmd === 'session_steer') {
      window.__calls.push([cmd, args, options]);
      return steerFails ? Promise.reject(steerFails) : Promise.resolve('Sent to “Queued A” now: it takes it into the turn it’s taking.');
    }
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(queueing ? { message: 'Queued in Codex.', opened: false, queued: 'q:' + args.text } : { message: 'Typed into its Ghostty tab.', opened: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (picked && s.key === picked.key) running(s);
          if (picked || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          running(s);
          picked = { key: s.key, lane: l.id };
        });
      });
      boardNow = board;
      return board;
    });
  };

  // The board as the engine has it, the session changed as `change` says.
  var push = function (change) {
    var b = JSON.parse(JSON.stringify(boardNow));
    b.generatedAt = Date.now();
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) Object.assign(s, change); }); });
    boardNow = b;
    window.__emit('board', b);
  };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // The messages on their way at the end of the conversation, each as (its words, what it says).
  var unsaid = function () {
    return all('.ca-sessinsp .ca-thread-said.is-ahead').map(function (el) {
      var meta = q('.ca-thread-meta .ca-meta', el);
      return { el: el, words: el.querySelector('.ca-thread-you').textContent.trim(), says: meta ? meta.textContent : '' };
    });
  };
  var sayingAll = function (words) { return unsaid().filter(function (u) { return u.words === words; }); };
  var saying = function (words) { return sayingAll(words)[0]; };
  // Send now on a held message, by its words.
  var sendNow = function (words) { var u = saying(words); return u && q('.ca-thread-sendnow', u.el); };
  var remove = function (words) { var u = saying(words); return u && q('.ca-thread-unqueue', u.el); };
  var edit = function (words) { var u = saying(words); return u && q('.ca-thread-editheld', u.el); };
  var send = function (words) {
    type(box(), words);
    key(box(), { key: 'Enter' });
    return until(function () { var u = saying(words); return u && u.says === 'Sent'; }, 5000, 'the message sent');
  };

  say('queued send check: waiting for the list');
  until(function () { return row(); }, 20000, 'no Claude Code session to make run')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Queued A' && !!box(); }, 5000, 'its panel');
    })
    .then(function () { return send('Also check the docs'); })
    .then(function () {
      check('a message that went says Sent', saying('Also check the docs').says === 'Sent');
      // Its harness holds it: it says so, whatever the session's quiet says.
      push({ queuedAt: Date.now(), status: 'idle', statusSince: Date.now() - 60e3 });
      return until(function () { return saying('Also check the docs').says === QUEUED; }, 5000, 'the message queued');
    })
    .then(function () {
      var u = saying('Also check the docs');
      check('one its harness holds says it’s queued until its current step ends', u.says === QUEUED, u.says);
      check('not that it may not have reached the session', !u.el.classList.contains('is-unconfirmed') && !q('.ca-thread-askrow', u.el));
      // Long after the quiet that would mark it, it's still queued.
      return wait(1500);
    })
    .then(function () {
      check('and stays so while it waits', saying('Also check the docs').says === QUEUED);
      // The turn ends and the queue goes in.
      push({ queuedAt: undefined, status: 'working', statusSince: Date.now(), lastPromptAt: Date.now() });
      return until(function () { var u = saying('Also check the docs'); return !u || u.says === 'Sent'; }, 5000, 'the message out of the queue');
    })
    .then(function () {
      var u = saying('Also check the docs');
      check('out of the queue, it’s sent', !u || u.says === 'Sent', u && u.says);
      // A prompt the board has from within a second of a send is taken for it: the next goes later.
      return wait(1500).then(function () { return send('And the changelog'); });
    })
    .then(function () {
      push({ queuedAt: Date.now() });
      return until(function () { var u = saying('And the changelog'); return u && u.says === QUEUED; }, 5000, 'the second message queued');
    })
    .then(function () {
      // Taken; the board lists what the harness holds from here on.
      push({ queuedAt: undefined, lastPromptAt: Date.now() });
      return wait(6000);
    })
    .then(function () {
      queueing = true;
      return send('Then the tests');
    })
    .then(function () {
      // The one sent from here shows once, from the board's list, by the id it went under.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'q:Then the tests', at: at, text: 'Then the tests' }] });
      return until(function () { return all('.ca-sessinsp .ca-thread-said.is-ahead[data-prompt^="queued:"]').length === 1; }, 5000, 'the board’s queued message');
    })
    .then(function () {
      var shown = sayingAll('Then the tests');
      check('a message the board lists as queued shows once', shown.length === 1, shown.length);
      check('as queued until its current step ends', shown[0] && shown[0].says === QUEUED, shown[0] && shown[0].says);
      // One sent into the turn (Send now) isn't queued, though another is queued after it.
      queueing = false;
      return wait(1500).then(function () { return send('Right away'); });
    })
    .then(function () {
      // One queued elsewhere (the Codex app's own box, another device) shows too, in the queue's order.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'q:Then the tests', at: at - 5000, text: 'Then the tests' }, { id: 'app-1', at: at, text: 'Queued in the app' }] });
      return until(function () { return !!saying('Queued in the app'); }, 5000, 'the message queued elsewhere');
    })
    .then(function () {
      var now = sayingAll('Right away');
      check('one sent into the turn still shows, as sent, once something is queued after it', now.length === 1 && now[0].says === 'Sent', now.map(function (u) { return u.says; }).join(', '));
      var u = saying('Queued in the app');
      check('one queued elsewhere shows, as queued', u.says === QUEUED, u.says);
      var said = all('.ca-sessinsp .ca-thread-said');
      var tail = said.slice(-2).map(function (el) { return el.querySelector('.ca-thread-you') && el.querySelector('.ca-thread-you').textContent.trim(); });
      check('both queued at the end of the conversation, in the queue’s order', tail[0] === 'Then the tests' && tail[1] === 'Queued in the app', tail.join(' | '));
      // The queue empties as the turn ends: neither waits any more.
      push({ queuedAt: undefined, queued: undefined, status: 'working', statusSince: Date.now(), lastPromptAt: Date.now() });
      return until(function () { return !saying('Queued in the app'); }, 5000, 'the queue emptied');
    })
    .then(function () {
      check('out of the queue, one queued elsewhere goes', !saying('Queued in the app'));
      var u = saying('Then the tests');
      check('and yours says sent, if it still shows', !u || u.says === 'Sent', u && u.says);
      return wait(6000);
    })
    .then(function () {
      push({ status: 'working', steerable: false });
      queueing = true;
      return send('Then the docs');
    })
    .then(function () {
      queueing = false;
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'q:Then the docs', at: at - 1000, text: 'Then the docs' }, { id: 'app-3', at: at, text: 'Queued in the app' }] });
      return until(function () { var u = saying('Then the docs'); return u && u.says === QUEUED && !!saying('Queued in the app'); }, 5000, 'two queued');
    })
    .then(function () {
      check('no Send now on what’s held while the turn can’t take it sooner', !sendNow('Then the docs') && !sendNow('Queued in the app'));
      push({ steerable: true });
      return until(function () { return !!sendNow('Then the docs'); }, 5000, 'Send now on what’s held');
    })
    .then(function () {
      check('Send now on each held message while the turn can take it sooner', !!sendNow('Then the docs') && !!sendNow('Queued in the app'));
      sendNow('Then the docs').click();
      return until(function () { var u = saying('Then the docs'); return u && u.says === 'Sent'; }, 5000, 'yours sent now');
    })
    .then(function () {
      var asked = sent('session_steer');
      check('Send now asks for the held message by its id', asked.length === 1 && asked[0].key === picked.key && asked[0].id === 'q:Then the docs', JSON.stringify(asked));
      var shown = sayingAll('Then the docs');
      check('one of yours sent now shows once, as sent, while the board still lists it', shown.length === 1 && shown[0].says === 'Sent', shown.map(function (u) { return u.says; }).join(', '));
      // Out of the queue; the turn takes it in at its next step.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-3', at: at, text: 'Queued in the app' }] });
      return wait(1500);
    })
    .then(function () {
      var shown = sayingAll('Then the docs');
      check('and once, sent, out of the queue until the record has it', shown.length === 1 && shown[0].says === 'Sent', shown.map(function (u) { return u.says; }).join(', '));
      sendNow('Queued in the app').click();
      return until(function () { var u = saying('Queued in the app'); return u && u.says === 'Sent'; }, 5000, 'one queued elsewhere sent now');
    })
    .then(function () {
      var shown = sayingAll('Queued in the app');
      check('one queued elsewhere sent now shows once, as sent', shown.length === 1 && shown[0].says === 'Sent', shown.map(function (u) { return u.says; }).join(', '));
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-2', at: at, text: 'Then the release notes' }] });
      return until(function () { return !!sendNow('Then the release notes'); }, 5000, 'another queued');
    })
    .then(function () {
      steerFails = 'Codex didn’t take it into the turn: its turn had ended.';
      sendNow('Then the release notes').click();
      return until(function () { var u = saying('Then the release notes'); return u && !!q('.ca-thread-askrow', u.el); }, 5000, 'why it didn’t go');
    })
    .then(function () {
      var u = saying('Then the release notes');
      var why = q('.ca-thread-askrow', u.el).textContent;
      check('one that didn’t go is still held, saying why', u.says === QUEUED && why.indexOf('its turn had ended') >= 0, u.says + ' / ' + why);
      steerFails = null;
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-4', at: at - 1000, text: 'Then the release notes' }, { id: 'app-5', at: at, text: 'And a line in the changelog' }] });
      return until(function () { return !!saying('And a line in the changelog'); }, 5000, 'two queued');
    })
    .then(function () {
      check('Remove beside Send now on each held message', !!remove('Then the release notes') && !!remove('And a line in the changelog'));
      remove('And a line in the changelog').click();
      return until(function () { return !saying('And a line in the changelog'); }, 5000, 'the removed one gone');
    })
    .then(function () {
      var asked = sent('session_unqueue');
      check('Remove asks for the held message by its id', asked.length === 1 && asked[0].key === picked.key && asked[0].id === 'app-5', JSON.stringify(asked));
      check('and it’s gone from the thread at once, the other still held', !saying('And a line in the changelog') && saying('Then the release notes').says === QUEUED);
      check('nothing else is done to what’s held while it’s taken out', remove('Then the release notes').disabled && sendNow('Then the release notes').disabled);
      unqueued.res({ message: 'Took it out of what “Queued A” holds: it won’t be sent.' });
      return wait(1500);
    })
    .then(function () {
      check('it stays gone while the board still lists it', !saying('And a line in the changelog'));
      check('and what’s held can be acted on again', !remove('Then the release notes').disabled && !sendNow('Then the release notes').disabled);
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-4', at: at - 1000, text: 'Then the release notes' }] });
      return wait(1500);
    })
    .then(function () {
      check('and once the board doesn’t', !saying('And a line in the changelog'));
      remove('Then the release notes').click();
      return until(function () { return !saying('Then the release notes'); }, 5000, 'the second taken out');
    })
    .then(function () {
      unqueued.rej('Claude Code didn’t give back what it holds for “Queued A”, so it’s still held.');
      return until(function () { var u = saying('Then the release notes'); return u && !!q('.ca-thread-askrow', u.el); }, 5000, 'why it wasn’t taken out');
    })
    .then(function () {
      var u = saying('Then the release notes');
      check('one that wasn’t taken out is back, held, saying why', u.says === QUEUED && q('.ca-thread-askrow', u.el).textContent.indexOf('still held') >= 0, u.says);
      queueing = true;
      return send('Fix the tests too');
    })
    .then(function () {
      queueing = false;
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-4', at: at - 1000, text: 'Then the release notes' }, { id: 'q:Fix the tests too', at: at, text: 'Fix the tests too' }] });
      return until(function () { var u = saying('Fix the tests too'); return u && u.says === QUEUED; }, 5000, 'yours held after it');
    })
    .then(function () {
      remove('Then the release notes').click();
      return until(function () { return !saying('Then the release notes'); }, 5000, 'taken out again');
    })
    .then(function () {
      // Claude Code gave back both; yours went in again under another id.
      unqueued.res({ message: 'Took it out of what “Queued A” holds.', requeued: [{ was: 'q:Fix the tests too', now: 'n:Fix the tests too' }] });
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'n:Fix the tests too', at: at, text: 'Fix the tests too' }] });
      return wait(1500);
    })
    .then(function () {
      var shown = sayingAll('Fix the tests too');
      check('one of yours that went back in under a new id shows once, as held', shown.length === 1 && shown[0].says === QUEUED, shown.map(function (u) { return u.says; }).join(', '));
      check('and the one taken out is gone', !saying('Then the release notes'));
      // Edit: buttons as icons, each named by its tooltip.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-6', at: at - 1000, text: 'Word this better' }, { id: 'n:Fix the tests too', at: at, text: 'Fix the tests too' }] });
      return until(function () { return !!saying('Word this better'); }, 5000, 'one to edit');
    })
    .then(function () {
      var acts = q('.ca-thread-heldacts', saying('Word this better').el);
      var buttons = [].slice.call(acts.querySelectorAll('button'));
      check('Send now, Edit and Remove, in that order', buttons.map(function (b) { return b.getAttribute('aria-label'); }).join() === 'Send now,Edit,Remove', buttons.length);
      check('as icons alone, with tooltips that name them', buttons.every(function (b) { return !b.textContent.trim() && !!q('svg', b) && b.title.indexOf(b.getAttribute('aria-label')) === 0; }), buttons.map(function (b) { return b.title; }).join(' | '));
      type(box(), 'Already written');
      edit('Word this better').click();
      return until(function () { return !saying('Word this better'); }, 5000, 'the edited one out of the queue');
    })
    .then(function () {
      var asked = sent('session_unqueue');
      check('Edit takes it out by its id', asked[asked.length - 1].id === 'app-6', JSON.stringify(asked[asked.length - 1]));
      check('and the box waits until it’s out', box().value === 'Already written', box().value);
      unqueued.res({ message: 'Took it out of what “Queued A” holds: it won’t be sent.' });
      return until(function () { return box().value.indexOf('Word this better') >= 0; }, 5000, 'its words in the box');
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('its words come after what was written', box().value === 'Already written\n\nWord this better', box().value);
      check('with the cursor at their end', document.activeElement === box() && box().selectionStart === box().value.length, box().selectionStart);
      // Words the box has in it already come all the same: they're nowhere else now.
      type(box(), 'Do not run tests yet');
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-11', at: at, text: 'run tests' }] });
      return until(function () { return !!saying('run tests'); }, 5000, 'one the box has in it');
    })
    .then(function () {
      edit('run tests').click();
      return until(function () { return !saying('run tests'); }, 5000, 'that one out of the queue');
    })
    .then(function () {
      unqueued.res({ message: 'Took it out.' });
      return until(function () { return box().value === 'Do not run tests yet\n\nrun tests'; }, 5000, 'its words after what has them in it');
    })
    .then(function () {
      check('one the box has in it already still comes', box().value === 'Do not run tests yet\n\nrun tests', box().value);
      type(box(), '');
      // While a side conversation has the box, as another window would show one.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-12', at: at, text: 'Not while it’s aside' }] });
      return until(function () { return !!saying('Not while it’s aside'); }, 5000, 'one with a side conversation up');
    })
    .then(function () {
      var view = 'codealong.sideview.' + picked.key;
      localStorage.setItem(view, JSON.stringify({ kind: 'list' }));
      window.dispatchEvent(new StorageEvent('storage', { key: view }));
      return until(function () { return edit('Not while it’s aside').disabled; }, 5000, 'Edit off while the box is aside');
    })
    .then(function () {
      var off = edit('Not while it’s aside');
      check('a side conversation that has the box turns Edit off, saying why', off.disabled && off.title.indexOf('side conversation') >= 0, off.title);
      check('and leaves Send now and Remove', !sendNow('Not while it’s aside').disabled && !remove('Not while it’s aside').disabled);
      var view = 'codealong.sideview.' + picked.key;
      localStorage.removeItem(view);
      window.dispatchEvent(new StorageEvent('storage', { key: view }));
      return until(function () { return !edit('Not while it’s aside').disabled; }, 5000, 'Edit back with the box');
    })
    .then(function () {
      // One of yours comes back as you wrote it, not as its harness lists it.
      queueing = true;
      return send('Yours, whole');
    })
    .then(function () {
      queueing = false;
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'q:Yours, whole', at: at, text: '[Pasted text #1 +40 lines]' }, { id: 'app-8', at: at, text: 'Terminal paste: [Pasted text #2 +9 lines]' }] });
      return until(function () { return !!saying('[Pasted text #1 +40 lines]') && !!saying('Terminal paste: [Pasted text #2 +9 lines]'); }, 5000, 'held by placeholders');
    })
    .then(function () {
      var whole = edit('Terminal paste: [Pasted text #2 +9 lines]');
      check('Edit is off where it only names a paste, saying why', whole.disabled && whole.title.indexOf('can’t bring back') >= 0, whole.title);
      check('and on for one of yours that it lists by a placeholder', !edit('[Pasted text #1 +40 lines]').disabled);
      edit('[Pasted text #1 +40 lines]').click();
      return until(function () { return !saying('[Pasted text #1 +40 lines]'); }, 5000, 'yours out of the queue');
    })
    .then(function () {
      unqueued.res({ message: 'Took it out.' });
      return until(function () { return box().value === 'Yours, whole'; }, 5000, 'your words in the box');
    })
    .then(function () {
      check('yours comes back as you wrote it', box().value === 'Yours, whole', box().value);
      type(box(), '');
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-9', at: at, text: 'Still held' }, { id: 'app-10', at: at, text: 'The turn takes this' }] });
      return until(function () { return !!saying('The turn takes this'); }, 5000, 'two more');
    })
    .then(function () {
      edit('Still held').click();
      return until(function () { return !saying('Still held'); }, 5000, 'out of sight');
    })
    .then(function () {
      unqueued.rej('Claude Code didn’t give back what it holds for “Queued A”, so it’s still held.');
      return until(function () { var u = saying('Still held'); return u && !!q('.ca-thread-askrow', u.el); }, 5000, 'why it wasn’t taken out');
    })
    .then(function () {
      check('one still held stays, saying why, the box left alone', q('.ca-thread-askrow', saying('Still held').el).textContent.indexOf('still held') >= 0 && box().value === '', box().value);
      edit('The turn takes this').click();
      return until(function () { return !saying('The turn takes this'); }, 5000, 'out of sight again');
    })
    .then(function () {
      unqueued.rej('Claude Code doesn’t hold that message any more: its turn has taken it in.');
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-9', at: at, text: 'Still held' }] });
      return until(function () { return !!q('.ca-sessinsp .ca-thread > .ca-thread-askrow .ca-btn--secondary'); }, 5000, 'Put in the box');
    })
    .then(function () {
      check('one the turn took first leaves the box alone', box().value === '', box().value);
      q('.ca-sessinsp .ca-thread > .ca-thread-askrow .ca-btn--secondary').click();
      return until(function () { return box().value === 'The turn takes this'; }, 5000, 'its words in the box');
    })
    .then(function () {
      check('Put in the box brings its words, and it’s said no more', !q('.ca-sessinsp .ca-thread > .ca-thread-askrow'));
      type(box(), '');
      var at = Date.now();
      push({ queuedAt: at, queued: [{ id: 'app-6', at: at - 1000, text: 'Then the release notes' }, { id: 'app-7', at: at, text: 'And a line in the changelog' }] });
      return until(function () { return !!saying('And a line in the changelog'); }, 5000, 'two queued for the screenshot');
    })
    .then(function () { return wait(300); })
    .then(done)
    .catch(stopped);
})();
