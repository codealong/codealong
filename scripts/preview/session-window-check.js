// Checks a session popped out of the main window's panel into a window of its own (kept.rs
// `pop_session`; SessionHeader.tsx's Pop out, SessionPanel.tsx `KeptAway`, kept.ts). In the main
// window: the header's Pop out asks the engine for its window, with the keyboard; once the engine
// says it's out, the panel keeps its header but shows, in its conversation's place, that it's in a
// window of its own, with no box and no Pop out; Show its window brings the window forward, as
// Show its window in its menu does; Put it back here asks for the window to close, and once the
// engine says so the conversation and box are back, and the menu offers Open in its own window
// again. In the kept window: its Main window, and its menu, put the session back in the main
// window, which comes forward on it, rather than showing it there too.
//   scripts/preview.sh sessions '' scripts/preview/session-window-check.js
//   scripts/preview.sh kept '' scripts/preview/session-window-check.js
(function () {
  (async function () {
    try {
      if (window.__CODEALONG_KEPT__) await inKept();
      else await inMain();
      done();
    } catch (e) {
      stopped(e);
    }
  })();

  async function inMain() {
    var panel = await until(function () { return q('.ca-sessinsp[data-shows-session]'); }, 30000, 'a session in the panel');
    var key = panel.getAttribute('data-shows-session');
    await until(function () { return q('.ca-sessinsp-popout', panel); }, 5000, 'Pop out in its header');
    check('its conversation shows, with Pop out in its header', !!q('.ca-thread', panel) && !q('.ca-sessinsp-away', panel));

    q('.ca-sessinsp-popout', panel).click();
    await until(function () { return calls('session_window').length; }, 3000, 'a window asked for');
    var asked = last('session_window');
    check('Pop out asks for a window of its own, with the keyboard', asked.key === key && asked.keys === true, JSON.stringify(asked));
    var away = await until(function () { return q('.ca-sessinsp-away', panel); }, 3000, 'its place, once it’s out');
    check('once it’s out, the panel says so where its conversation was', /window of its own/.test(away.textContent) && !q('.ca-thread', panel), away.textContent);
    check('with its header still there, and no box or Pop out', !!q('.ca-sessinsp-title', panel) && !q('.ca-composer-field textarea', panel) && !q('.ca-sessinsp-popout', panel));

    var before = calls('session_window').length;
    button(away, /Show its window/).click();
    await until(function () { return calls('session_window').length > before; }, 3000, 'its window brought forward');
    check('Show its window brings its window forward, with the keyboard', last('session_window').key === key && last('session_window').keys === true);

    var labels = await menuOf(q('.ca-sessinsp-top', panel));
    check('its menu offers Show its window, not to pop it out again', labels.indexOf('Show its window') >= 0 && labels.indexOf('Open in its own window') < 0, labels.join(' | '));

    button(away, /Put it back here/).click();
    await until(function () { return calls('session_put_back').length; }, 3000, 'put back');
    var back = last('session_put_back');
    check('Put it back here asks for its window to close, bringing nothing forward', back.key === key && !back.reveal, JSON.stringify(back));
    await until(function () { return !q('.ca-sessinsp-away', panel) && q('.ca-thread', panel); }, 3000, 'its conversation back');
    check('once it’s back, so are its conversation, its box and Pop out', !!q('.ca-composer-field textarea', panel) && !!q('.ca-sessinsp-popout', panel));

    labels = await menuOf(q('.ca-sessinsp-top', panel));
    check('its menu offers Open in its own window again', labels.indexOf('Open in its own window') >= 0, labels.join(' | '));
    before = calls('session_window').length;
    menuItem('Open in its own window')();
    await until(function () { return calls('session_window').length > before; }, 3000, 'the menu’s window');
    check('which pops it out too', last('session_window').key === key);
  }

  async function inKept() {
    var card = await until(function () { return q('[data-shows-session]'); }, 30000, 'the kept session');
    var key = card.getAttribute('data-shows-session');
    var toMain = await until(function () { return q('.ca-sessinsp-tomain', card); }, 5000, 'Main window in its header');
    check('Main window says it puts the session back', /Put back in main window/.test(toMain.getAttribute('aria-label') || ''), toMain.getAttribute('aria-label'));
    toMain.click();
    await until(function () { return calls('session_put_back').length; }, 3000, 'put back');
    var back = last('session_put_back');
    check('it closes this window and brings the main window forward on the session', back.key === key && back.reveal && back.reveal.kind === 'session' && back.reveal.key === key, JSON.stringify(back));
    check('rather than showing it there too', !calls('show_main').length);
    var labels = await menuOf(q('.ca-sessinsp-top', card));
    check('its menu says Put back in main window too', labels.indexOf('Put back in main window') >= 0 && labels.indexOf('Open in main window') < 0, labels.join(' | '));
  }

  function button(root, words) {
    var b = all('button', root).filter(function (x) { return words.test(x.textContent); })[0];
    if (!b) throw new Error('no button ' + words);
    return b;
  }
})();
