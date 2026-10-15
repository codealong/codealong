// Checks a side conversation closed while its copy answers (side.rs `close`, side.ts
// `useClosing`, SideStrip.tsx), in the bar's peek: Close gives the box back to the session at
// once, and the answer isn't cut off; the session's header names it, still answering; a click on
// it there shows it to read, saying it's closed and still answering, its answer growing as it
// comes; Earlier lists it as answering; a question asked meanwhile starts a new side conversation
// rather than joining it; and once it has answered, it reads as a kept one, its answer in it.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/side-closing-check.js
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
  localStorage.removeItem('codealong.draft.side:' + key);
  localStorage.removeItem('codealong.sideview.' + key);
  // None kept to start with: the board lists only the ones this check keeps.
  window.__past = [];
  window.__closing = {};
  var calm = { status: 'idle', live: true, stoppable: false, stalledSince: null, asks: false };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) Object.assign(s, calm); }); });
      return b;
    });
  };

  var box = function () { return document.querySelector('.ca-composer textarea'); };
  var strip = function () { return document.querySelector('.ca-sidestrip'); };
  var turns = function () { return all('.ca-sidestrip-turn').map(function (t) { return t.textContent; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var headButton = function (label) { return all('.ca-sidestrip-head button', strip()).filter(function (b) { return b.getAttribute('aria-label') === label; })[0]; };
  var sideLines = function () { return all('.ca-sessinsp-head .ca-sessinsp-kin.is-side'); };
  var sideLine = function (text) { return sideLines().filter(function (b) { return b.textContent.indexOf(text) >= 0; })[0]; };
  var aside = function () { return !!q('.ca-composer.is-aside'); };

  (async function () {
    try {
      await until(box, 20000, 'the peek’s box');
      type(box(), '/side why a queue?');
      await wait(50);
      enter();
      await until(function () { return strip() && turns().length === 1; }, 5000, 'the question above the box');
      var first = window.__sides[key];
      var at = first.turns[0].at;
      // The copy is at work on it.
      var working = { id: first.id, key: key, turns: [{ question: 'why a queue?', answer: '', tool: 'Bash · `git log -3`', done: false, at: at }], running: true, ended: false, startedAt: first.startedAt };
      window.__sides[key] = working;
      window.__emit('side', working);
      await until(function () { return turns()[0].indexOf('git log -3') > 0; }, 5000, 'the tool it runs');

      headButton('Close').click();
      await until(function () { return !strip(); }, 5000, 'the strip gone');
      // The board as the engine makes it again once it's closed, which lists it as closed.
      await until(function () { return window.__closing[first.id]; }, 5000, 'it closed');
      await window.__pushSides();
      check('Close asks the engine to close it', calls('side_close').length === 1 && calls('side_close')[0][1].key === key);
      check('and gives the box back to the session at once', !aside() && box().getAttribute('aria-placeholder') !== 'Ask on the side', box().getAttribute('aria-placeholder'));
      await until(function () { return sideLines().length === 1; }, 5000, 'the header naming it');
      check('the session’s header names it, still answering', /why a queue\?/.test(sideLines()[0].textContent) && !!q('.ca-glyph--working', sideLines()[0]), sideLines()[0].innerHTML.slice(0, 200));

      // Its answer starts to come while it's closed.
      var closed = function (answer, running) {
        var side = { id: first.id, key: key, turns: [{ question: 'why a queue?', answer: answer, tools: ['Bash · `git log -3`'], done: !running, at: at }], running: running, ended: false, closed: true, startedAt: first.startedAt };
        if (running) window.__closing[first.id] = side;
        else {
          delete window.__closing[first.id];
          window.__past.push({ id: first.id, key: key, title: '', startedAt: first.startedAt, lastAt: Date.now(), turns: side.turns });
        }
        window.__emit('side', side);
      };
      closed('Because sends ', true);
      await wait(50);
      sideLines()[0].click();
      await until(function () { return strip() && turns().length === 1; }, 5000, 'it shown to read');
      check('a click on it shows it to read, closed and still answering', /Closed, and still answering/.test(strip().textContent) && turns()[0].indexOf('Because sends') > 0 && !/Closed before/.test(strip().textContent), strip().textContent.slice(0, 200));
      check('while it shows, the box asks on the side', aside() && box().getAttribute('aria-placeholder') === 'Ask on the side');
      closed('Because sends must keep their **order**.', true);
      await until(function () { return turns()[0].indexOf('must keep their order') > 0; }, 5000, 'more of its answer');
      check('its answer grows as it comes', !!q('.ca-sidestrip-a strong', strip()), turns()[0]);
      check('Delete says it stops it too', /^Stop it answering/.test(headButton('Delete').title), headButton('Delete').title);

      headButton('Earlier').click();
      await until(function () { return all('.ca-sidestrip-kept', strip()).length === 1; }, 5000, 'the list');
      var row = q('.ca-sidestrip-kept', strip()).textContent;
      check('Earlier lists it as answering', row.indexOf('why a queue?') === 0 && /answering · 1 question/.test(row), row);

      // A question meanwhile starts a new side conversation; the closed one answers on.
      type(box(), 'and without one?');
      await wait(50);
      enter();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('and without one?') === 0; }, 5000, 'the new question above the box');
      check('a question meanwhile starts a new side conversation', calls('side_ask').length === 2 && window.__sides[key].id !== first.id && !/still answering/.test(strip().textContent), JSON.stringify(window.__sides[key]).slice(0, 200));

      // The closed one has answered: it's kept, its answer in it.
      closed('Because sends must keep their **order**.', false);
      await window.__pushSides();
      await until(function () { return !!headButton('Earlier'); }, 5000, 'Earlier beside the new one');
      headButton('Earlier').click();
      await until(function () { return all('.ca-sidestrip-kept', strip()).length === 1 && !/answering/.test(q('.ca-sidestrip-kept', strip()).textContent); }, 5000, 'it listed as answered');
      q('.ca-sidestrip-kept', strip()).click();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('why a queue?') === 0; }, 5000, 'it shown to read');
      check('once answered it reads as a kept one, its answer in it', /closed\. A new question/.test(strip().textContent) && turns()[0].indexOf('must keep their order') > 0 && !/still answering/.test(strip().textContent), strip().textContent.slice(0, 200));
      await until(function () { return !!sideLine('why a queue?') || !!q('.ca-sessinsp-head .ca-sessinsp-kids-more'); }, 5000, 'the header');
      var line = sideLine('why a queue?');
      check('and the header no longer says it’s answering', !line || !q('.ca-glyph--working', line), line && line.innerHTML.slice(0, 200));

      // What the engine said of it before it stopped, arriving late (a window's close is told from
      // another thread than its copy's lines), changes nothing.
      window.__emit('side', { id: first.id, key: key, turns: [{ question: 'why a queue?', answer: 'Because sends ', done: false, at: at }], running: true, ended: false, closed: true });
      await wait(100);
      check('word of it answering that comes after it stopped is too late to change it', /closed\. A new question/.test(strip().textContent) && turns()[0].indexOf('must keep their order') > 0, strip().textContent.slice(0, 200));

      // One closed and answering before this window heard of it: read as answering all the same.
      window.__closing.pre = { id: 'pre', key: key, turns: [{ question: 'what ran last?', answer: '', tool: 'Read · `send.rs`', done: false, at: Date.now() }], running: true, ended: false, closed: true, startedAt: Date.now() };
      await window.__pushSides();
      await until(function () { return !!sideLine('what ran last?'); }, 5000, 'its line in the header');
      sideLine('what ran last?').click();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('what ran last?') === 0; }, 5000, 'it shown to read');
      check('one closed before the window heard of it reads as still answering, at the tool it runs', /Closed, and still answering/.test(strip().textContent) && turns()[0].indexOf('Read · send.rs') > 0 && turns()[0].indexOf('Next') < 0, strip().textContent.slice(0, 200));

      // Another window closes the open one, and word of it comes here in an order this window
      // didn't start: an open copy sent before the close arrives after it.
      var second = window.__sides[key];
      headButton('Close').click();
      await until(function () { return turns().length === 1 && turns()[0].indexOf('and without one?') === 0; }, 5000, 'the open one above the box');
      delete window.__sides[key];
      window.__emit('side', { id: '', key: key, turns: [], running: false, ended: false });
      window.__emit('side', second);
      window.__emit('side', Object.assign({}, second, { closed: true }));
      await wait(100);
      check('closed in another window, an open copy of it heard late doesn’t keep it above the box', !strip() && !aside(), strip() && strip().textContent.slice(0, 200));
      window.__emit('side', Object.assign({}, second, { closed: true, running: false, turns: second.turns.map(function (t) { return Object.assign({}, t, { answer: 'The same.', done: true }); }) }));
      await wait(100);
      check('nor once it has answered, and the box is the session’s', !strip() && !aside(), strip() && strip().textContent.slice(0, 200));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
