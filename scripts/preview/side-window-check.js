// Checks a side conversation popped out into a window of its own (SideStrip.tsx, SessionReply.tsx
// `SideReply`, Peek.tsx `side`, side.ts `usePopped`). In the bar's peek: the strip's actions are
// icons, each named on hover; Pop out asks the engine for its window, with the keyboard, and once
// the engine says it's out the strip leaves the box, which is the session's again; its line in the
// header brings the window forward; `/side` in the session's box still asks the copy and brings
// the window forward without the keyboard, and plain text goes to the session; and once the
// window closes, it's back above the box, which is the side conversation's again. In the window:
// it says what session it's about, whose title shows the session in Codealong's main window;
// with nothing open it says so over a box that asks on the side; a question asks the copy and
// sends the session nothing; Put back closes the window and leaves the copy be; Close and ⌘W close
// it and the window; Earlier lists the ones from before, the ones closed while their copies
// answered still answering, and Back goes back.
//   scripts/preview.sh peek '' scripts/preview/side-window-check.js
//   scripts/preview.sh side '' scripts/preview/side-window-check.js
(function () {
  // A Claude Code session, idle and taking messages.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to show'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  localStorage.removeItem('codealong.draft.side:' + key);
  localStorage.removeItem('codealong.sideview.' + key);
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
  var inWindow = window.__CODEALONG_SIDE__ === true;

  var box = function () { return q('.ca-composer textarea'); };
  var noteText = function () { var n = q('.ca-composer-note'); return n ? n.textContent : ''; };
  var strip = function () { return q('.ca-sidestrip'); };
  var turns = function () { return all('.ca-sidestrip-turn').map(function (t) { return t.textContent; }); };
  var aside = function () { return !!q('.ca-composer.is-aside'); };
  var enter = function (meta) { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: !!meta, bubbles: true, cancelable: true })); };
  var acts = function () { return strip() ? all('.ca-sidestrip-acts button', strip()) : []; };
  var act = function (label) { return acts().filter(function (b) { return b.getAttribute('aria-label') === label; })[0]; };
  var labels = function () { return acts().map(function (b) { return b.getAttribute('aria-label'); }).join(', '); };
  var ask = async function (text) {
    var before = calls('side_ask').length;
    type(box(), text);
    await wait(50);
    enter();
    await until(function () { return calls('side_ask').length > before && turns().length > 0; }, 5000, 'the question');
  };
  var past = function () {
    window.__past = [{ id: 'old', key: key, title: '', startedAt: Date.now() - 7200000, lastAt: Date.now() - 7200000, turns: [{ question: 'is the queue tested?', answer: 'In `send.rs`.', done: true }] }];
    window.__pushSides();
  };

  async function inPeek() {
    past();
    await until(box, 20000);
    // The header's line for the open one, the latest.
    var line = function () { return all('.ca-sessinsp-head .ca-sessinsp-kin.is-side').filter(function (b) { return b.textContent.indexOf('why a queue?') >= 0; })[0]; };
    await ask('/side why a queue?');
    check('the strip’s actions are icons, each named on hover', acts().length === 3 && acts().every(function (b) { return !b.textContent.trim() && !!b.querySelector('svg') && !!b.title; }) && labels() === 'Earlier, Pop out, Close', labels());
    var sends = calls('send_to_session').length;

    act('Pop out').click();
    await until(function () { return calls('side_window').length === 1; }, 5000, 'Pop out');
    var popped = calls('side_window')[0][1];
    check('Pop out asks for its window, with the keyboard', popped.key === key && popped.keys === true, JSON.stringify(popped));
    await until(function () { return !strip(); }, 5000, 'the strip to leave');
    check('once it’s out, the box is the session’s again', !aside() && box().getAttribute('aria-placeholder') !== 'Ask on the side', box().getAttribute('aria-placeholder'));
    await until(line, 5000, 'the header’s line');
    line().click();
    await until(function () { return calls('side_window').length === 2; }, 5000, 'the line’s click');
    check('and brings the window forward', calls('side_window')[1][1].keys === true && !strip());

    var asked = calls('side_ask').length;
    type(box(), '/side and without one?');
    await wait(50);
    enter();
    await until(function () { return calls('side_ask').length > asked; }, 5000, '/side');
    await wait(50);
    var raised = calls('side_window')[2];
    check('/side in the session’s box asks the copy', calls('side_ask')[asked][1].text === 'and without one?' && calls('send_to_session').length === sends);
    check('and brings its window forward, leaving the keyboard here', !!raised && raised[1].keys === false && !strip() && !aside(), JSON.stringify(raised && raised[1]));
    type(box(), 'to the session');
    await wait(50);
    enter();
    await until(function () { return calls('send_to_session').length > sends; }, 5000, 'a send');
    check('plain text goes to the session', calls('side_ask').length === asked + 1);

    // The window closes: it's back above the box.
    window.__sideWindows = [];
    window.__emit('side-windows', []);
    await until(strip, 5000, 'the strip to come back');
    check('once its window closes, it’s back above the box, which asks it', turns().length === 2 && aside() && labels() === 'Earlier, Pop out, Close', labels());
  }

  async function ownWindow() {
    await until(function () { return strip() && box(); }, 20000, 'the window');
    check('it says what session it’s about', q('.ca-sidestrip-about').textContent === picked.title, q('.ca-sidestrip-about').textContent);
    var title = q('.ca-sidestrip-head .ca-sidestrip-session');
    title.click();
    await until(function () { return calls('show_main').length === 1; }, 5000, 'the main window');
    var shown = calls('show_main')[0][1];
    check('a click on its title shows the session in Codealong, and the window stays', !!title && shown.reveal && shown.reveal.kind === 'session' && shown.reveal.key === key && calls('kept_close').length === 0, JSON.stringify(shown));
    check('with nothing open it says so', /^Nothing open on the side/.test(q('.ca-sidestrip-doing').textContent) && labels() === 'Close', labels());
    check('over a box that asks on the side', aside() && box().getAttribute('aria-placeholder') === 'Ask on the side' && /throwaway copy/.test(noteText()), noteText());
    await ask('why a queue?');
    check('a question asks the copy, and sends the session nothing', calls('side_ask')[0][1].text === 'why a queue?' && calls('send_to_session').length === 0 && turns()[0].indexOf('why a queue?') === 0);
    check('the box is still the side conversation’s', aside() && box().value === '');
    check('its actions: put back, and close', labels() === 'Put back, Close', labels());

    act('Put back').click();
    await until(function () { return calls('kept_close').length === 1; }, 5000, 'Put back');
    check('Put back closes the window and leaves the copy be', calls('side_close').length === 0);

    act('Close').click();
    await until(function () { return calls('kept_close').length === 2; }, 5000, 'Close');
    check('Close closes it and the window', calls('side_close').length === 1 && calls('side_close')[0][1].key === key);
    await until(function () { return q('.ca-sidestrip-doing') && /^Nothing open/.test(q('.ca-sidestrip-doing').textContent); }, 5000, 'nothing open');

    await ask('once more?');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', metaKey: true, bubbles: true, cancelable: true }));
    await until(function () { return calls('kept_close').length === 3; }, 5000, '⌘W');
    check('⌘W does as Close does', calls('side_close').length === 2);

    past();
    await ask('and again?');
    await until(function () { return !!act('Earlier'); }, 5000, 'Earlier');
    act('Earlier').click();
    await until(function () { return all('.ca-sidestrip-kept').length === 3; }, 5000, 'the list');
    var rows = all('.ca-sidestrip-kept').map(function (r) { return r.textContent; });
    check('Earlier lists the ones from before, in the window, the two closed while they answered still answering', labels() === 'Back, Put back, Close' && rows.filter(function (t) { return /answering · 1 question/.test(t); }).length === 2 && /^is the queue tested\?/.test(rows[2]), rows.join(' | ') + ' / ' + labels());
    act('Back').click();
    await until(function () { return turns().length === 1; }, 5000, 'Back');
    check('Back goes back to the open one', turns()[0].indexOf('and again?') === 0);
  }

  (async function () {
    try {
      await (inWindow ? ownWindow() : inPeek());
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
