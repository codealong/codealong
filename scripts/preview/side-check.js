// Checks /side in the reply box (SessionReply.tsx, slash.ts, SideStrip.tsx), in the bar's peek:
// typing `/s` lists /side with what it does; ⏎ on `/btw <question>` asks a copy of the session, sends the
// session nothing, clears the box and shows the question above it; the answer shows as the engine
// tells of it (the tool the copy runs, then its words, and what it did folded under the answer);
// a second question joins the first; while it shows, the box is its, and plain text asks the copy
// rather than the session; Close ends it, keeps it, and gives the box back to the session; a
// question that can't be asked
// is back in the box with why; /side alone lists the ones kept, each opens to read, Delete
// forgets one; Earlier beside an open one lists them too; and the session's header says how many
// it has, which shows the only one, or the list, and puts it away again.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/side-check.js
(function () {
  // A Claude Code session to peek at, idle and taking messages.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to peek at'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  var calm = { status: 'idle', live: true, stoppable: false, stalledSince: null, asks: false };
  var failNext = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'side_ask' && failNext) {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.reject('Its folder is gone, so a copy of it has nowhere to run.');
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, calm); }); });
      return b;
    });
  };

  var box = function () { return document.querySelector('.ca-composer textarea'); };
  var button = function () { return document.querySelector('.ca-composer-send'); };
  var noteText = function () { var n = document.querySelector('.ca-composer-note'); return n ? n.textContent : ''; };
  var strip = function () { return document.querySelector('.ca-sidestrip'); };
  var turns = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-sidestrip-turn'), function (t) { return t.textContent; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  // What the engine would say, which the stub's next answer goes on from too.
  var tell = function (side) {
    side = Object.assign({ id: (window.__sides && window.__sides[key] || {}).id || 'side-told', key: key, running: true, ended: false }, side);
    window.__sides = window.__sides || {};
    window.__sides[key] = side;
    window.__emit('side', side);
  };

  (async function () {
    try {
      await until(box, 20000);
      var chip = function () { return document.querySelector('.ca-sessinsp-head .ca-sessinsp-sides'); };
      check('no side conversation shows until one is asked', !strip() && !chip());
      type(box(), '/s');
      await wait(50);
      var listed = Array.prototype.map.call(document.querySelectorAll('.ca-slash-row'), function (r) { return r.textContent; });
      check('a / and the start of its name lists /side with what it does', listed.some(function (t) { return /^\/side \[question\]Codealong: /.test(t); }), listed.join(' | '));
      type(box(), '/btw why a queue?');
      await wait(50);
      check('/btw is the same command', button().getAttribute('aria-label') === 'Run /side' && !button().disabled, button().getAttribute('aria-label'));
      var sends = calls('send_to_session').length;
      var hides = calls('peek_hide').length;
      enter();
      await until(strip, 5000);
      var asks = calls('side_ask');
      check('⏎ asks a copy of the session', asks.length === 1 && asks[0][1].key === key && asks[0][1].text === 'why a queue?', JSON.stringify(asks.map(function (c) { return c[1]; })));
      check('and sends the session nothing', calls('send_to_session').length === sends);
      check('clears the box', box().value === '', box().value);
      check('leaves the peek open', calls('peek_hide').length === hides);
      check('shows the question above the box', turns().length === 1 && turns()[0].indexOf('why a queue?') === 0 && turns()[0].indexOf('Reading the conversation') > 0, turns().join(' / '));
      check('the strip sits right above the box', strip().nextElementSibling === document.querySelector('.ca-composer:last-child'));

      tell({ turns: [{ question: 'why a queue?', answer: '', tool: 'Bash · `git log -3`', done: false }] });
      await wait(50);
      check('shows the tool the copy runs', turns()[0].indexOf('Bash · git log -3') > 0 && turns()[0].indexOf('Reading') < 0, turns()[0]);
      tell({ turns: [{ question: 'why a queue?', answer: 'Because sends must keep their **order**.', done: false }] });
      await wait(50);
      check('then its answer as it comes', turns()[0].indexOf('Because sends must keep their order.') > 0 && !!strip().querySelector('.ca-sidestrip-a strong'), turns()[0]);
      tell({ running: false, turns: [{ question: 'why a queue?', answer: 'Because sends must keep their **order**.', tools: ['Bash · `git log -3`', 'Read · `send.rs`'], done: true }] });
      await wait(50);
      check('and nothing more once it has answered', !strip().querySelector('.ca-sidestrip-doing'));
      var steps = strip().querySelector('.ca-sidestrip-steps');
      check('but what it did, folded', !!steps && !steps.open && steps.querySelector('summary').textContent === '2 steps' && steps.textContent.indexOf('Read · send.rs') > 0, steps && steps.textContent);

      type(box(), '/side and without one?');
      await wait(50);
      enter();
      await until(function () { return turns().length === 2; }, 5000);
      check('a second question joins the first', calls('side_ask')[1][1].text === 'and without one?' && turns()[0].indexOf('order') > 0, turns().join(' / '));
      tell({ turns: [{ question: 'why a queue?', answer: 'Because.', done: true }, { question: 'and without one?', answer: '', done: true, error: 'The copy stopped: usage limit reached' }], running: false, ended: true });
      await wait(50);
      check('a copy that stops says why', turns()[1].indexOf('The copy stopped: usage limit reached') > 0 && strip().textContent.indexOf('Its copy has ended') > 0, strip().textContent);

      // While it shows, the box is the side conversation's.
      check('while it shows, the box says it asks on the side', box().getAttribute('aria-placeholder') === 'Ask on the side' && button().getAttribute('aria-label') === 'Ask on the side' && noteText().indexOf('To the side conversation, not ') === 0 && !!document.querySelector('.ca-composer.is-aside'), box().getAttribute('aria-placeholder') + ' / ' + noteText());
      type(box(), 'and with a queue of one?');
      await wait(50);
      enter();
      await until(function () { return calls('side_ask').length === 3; }, 5000);
      check('and plain text asks the copy, not the session', calls('side_ask')[2][1].text === 'and with a queue of one?' && calls('send_to_session').length === sends && box().value === '', JSON.stringify(calls('side_ask')[2][1]));
      tell({ turns: [{ question: 'why a queue?', answer: 'Because.', done: true }, { question: 'and without one?', answer: '', done: true, error: 'The copy stopped: usage limit reached' }, { question: 'and with a queue of one?', answer: 'The same.', done: true }], running: false, ended: false });
      await wait(50);

      var headButton = function (label) { return Array.prototype.filter.call(strip().querySelectorAll('.ca-sidestrip-head button'), function (b) { return b.textContent.trim() === label; })[0]; };
      check('with none from before, no way to them shows', !headButton('Earlier'));
      headButton('Close').click();
      await until(function () { return !strip(); }, 5000);
      check('Close ends it', calls('side_close').length === 1 && calls('side_close')[0][1].key === key);
      check('and gives the box back to the session', box().getAttribute('aria-placeholder') !== 'Ask on the side' && !document.querySelector('.ca-composer.is-aside'), box().getAttribute('aria-placeholder'));
      type(box(), 'now to the session');
      await wait(50);
      enter();
      await until(function () { return calls('send_to_session').length > sends; }, 5000);
      check('which plain text goes to again', calls('side_ask').length === 3);

      await until(function () { return chip() && chip().textContent === '1 side conversation'; }, 5000);
      check('the header says the session has one kept', chip().getAttribute('aria-pressed') === 'false');
      chip().click();
      await until(function () { return turns().length === 3; }, 5000);
      check('which a click shows, to read', turns()[0].indexOf('why a queue?') === 0 && strip().textContent.indexOf('closed') > 0 && chip().getAttribute('aria-pressed') === 'true', strip().textContent.slice(0, 120));
      chip().click();
      await until(function () { return !strip(); }, 5000);
      check('and another puts away', chip().getAttribute('aria-pressed') === 'false' && calls('side_ask').length === 3);

      // What was closed is kept, with one from before.
      window.__past.push({ id: 'old', key: key, title: 'x', startedAt: Date.now() - 7200000, lastAt: Date.now() - 7200000, turns: [{ question: 'is the queue tested?\nand where?', answer: 'In `send.rs`.', tools: ['Grep · `in_order`'], done: true }] });
      var sentBefore = calls('send_to_session').length;
      var askedBefore = calls('side_ask').length;
      type(box(), '/side');
      await wait(50);
      enter();
      await until(function () { return strip() && strip().querySelectorAll('.ca-sidestrip-kept').length === 2; }, 5000);
      var rows = Array.prototype.map.call(strip().querySelectorAll('.ca-sidestrip-kept'), function (r) { return r.textContent; });
      check('/side alone lists the ones kept, the latest first', rows[0].indexOf('why a queue?') === 0 && rows[0].indexOf('3 questions') > 0 && rows[1].indexOf('is the queue tested?') === 0 && rows[1].indexOf('and where') < 0 && rows[1].indexOf('2h ago · 1 question') > 0, rows.join(' / '));
      check('asking and sending nothing', calls('side_ask').length === askedBefore && calls('send_to_session').length === sentBefore && box().value === '', box().value);
      check('the header counts both, and is lit while they show', chip().textContent === '2 side conversations' && chip().getAttribute('aria-pressed') === 'true', chip().textContent);
      check('while the list shows, the box asks on the side too', box().getAttribute('aria-placeholder') === 'Ask on the side');
      strip().querySelectorAll('.ca-sidestrip-kept')[1].click();
      await until(function () { return turns().length === 1; }, 5000);
      check('one opens to read', turns()[0].indexOf('is the queue tested?') === 0 && turns()[0].indexOf('In send.rs.') > 0 && strip().textContent.indexOf('closed') > 0 && !!strip().querySelector('.ca-sidestrip-steps'), strip().textContent);
      headButton('Delete').click();
      await until(function () { return strip().querySelectorAll('.ca-sidestrip-kept').length === 1; }, 5000);
      check('Delete forgets it and goes back to the list', calls('side_forget').length === 1 && calls('side_forget')[0][1].id === 'old');
      headButton('Close').click();
      await until(function () { return !strip(); }, 5000);
      check('Close puts the list away', calls('side_close').length === 1);
      check('the header counts the one that’s left', chip().textContent === '1 side conversation' && chip().getAttribute('aria-pressed') === 'false', chip().textContent);

      type(box(), '/side once more?');
      await wait(50);
      enter();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('once more?') === 0; }, 5000);
      await until(function () { return !!headButton('Earlier'); }, 5000);
      headButton('Earlier').click();
      await until(function () { return strip().querySelectorAll('.ca-sidestrip-kept').length === 1; }, 5000);
      check('Earlier, beside an open one, lists the ones before it', strip().querySelector('.ca-sidestrip-kept').textContent.indexOf('why a queue?') === 0);
      headButton('Close').click();
      await until(function () { return turns().length === 1; }, 5000);
      check('and Close goes back to the open one', turns()[0].indexOf('once more?') === 0 && calls('side_close').length === 1);
      chip().click();
      await until(function () { return strip().querySelectorAll('.ca-sidestrip-kept').length === 1; }, 5000);
      check('with one open, the header’s count lists the ones before it', chip().getAttribute('aria-pressed') === 'true');
      chip().click();
      await until(function () { return turns().length === 1; }, 5000);
      headButton('Close').click();
      await until(function () { return !strip(); }, 5000);

      failNext = true;
      type(box(), '/side where does it run?');
      await wait(50);
      enter();
      await until(function () { return noteText().indexOf('Its folder is gone') >= 0; }, 5000);
      check('a question that can’t be asked says why', !strip());
      check('and is back in the box', box().value === '/side where does it run?', box().value);
      failNext = false;

    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
