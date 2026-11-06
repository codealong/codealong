// Checks Main window in the header of a session shown as a card (SessionHeader.tsx): in the notch's
// session it shows its words, in the peek, which is narrower, its icon alone; a click brings
// Codealong's window forward on the session (show_main), and the ⋯ menu has the same as Open in
// main window. Every session is one Codealong doesn't run here, whose Open button would go there.
//   PREVIEW_SIZE=1512,720 scripts/preview.sh notch '' scripts/preview/card-main-window-check.js
//   scripts/preview.sh peek '' scripts/preview/card-main-window-check.js
(function () {
  var peek = window.__CODEALONG_PEEK__ === true;
  // The board's and your other devices' alike: the notch reads its session from either
  // (session_at), whichever the first pill is.
  var real = window.__TAURI_INTERNALS__.invoke;
  var unhosted = function (lanes) {
    lanes.forEach(function (l) {
      l.sessions = l.sessions.map(function (s) { return Object.assign({}, s, { hosted: false }); });
    });
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd === 'get_board' || cmd === 'devices') {
      return p.then(function (b) {
        if (b && b.lanes) unhosted(b.lanes);
        return b;
      });
    }
    if (cmd === 'session_at') {
      return p.then(function (at) { return at ? Object.assign({}, at, { session: Object.assign({}, at.session, { hosted: false }) }) : at; });
    }
    return p;
  };
  var button = function () { return q('.ca-sessinsp-tomain'); };
  var head = function () { return q('.ca-sessinsp-head'); };

  say('card main window check: waiting for a session');
  var opened = false;
  // The notch opens on a click, and then on its first pill's session; the peek opens by itself.
  until(function () {
    if (!peek && !opened && q('.ca-notch-shape') && q('.ca-notch-count')) {
      opened = true;
      q('.ca-notch-shape').click();
    }
    var pill = !peek && q('.ca-notch-body .ca-hud-pill[data-key]');
    if (pill && !head()) pill.click();
    return !!head();
  }, 20000, 'a session in the card')
    .then(function () { return wait(500); })
    .then(function () {
      var b = button();
      var tools = q('.ca-sessinsp-tools');
      check('the header has Main window', !!b, (q('.ca-sessinsp-title') || {}).textContent + ': ' + (tools ? tools.innerText : 'no tools'));
      if (!b) return done();
      var words = q(':scope > span', b);
      var shown = !!words && getComputedStyle(words).display !== 'none';
      var width = Math.round(b.getBoundingClientRect().width);
      if (peek) check('in the peek it is its icon alone', !shown && width <= 26, width + 'px');
      else check('in the notch it says Main window', shown && words.textContent === 'Main window', width + 'px');
      check('it says what it does', b.getAttribute('aria-label') === 'Open in main window' && /comes forward/.test(b.title), b.title);
      check('it sits beside the ways to open it', !!b.closest('.ca-sessinsp-tools'));
      return menuOf(head());
    })
    .then(function (labels) {
      if (!labels) return;
      // (A session that runs in Codealong opens there by its own item too, its worktree's Show in
      // Codealong, which stays.)
      check('the ⋯ menu says Open in main window', labels.indexOf('Open in main window') >= 0, labels.join(' | '));
      var before = calls('show_main').length;
      button().click();
      return until(function () { return calls('show_main').length > before; }, 3000, 'show_main').then(function () {
        var reveal = (last('show_main') || {}).reveal || {};
        check('a click brings up the main window on the session', reveal.kind === 'session' && !!reveal.key && !!reveal.laneId, JSON.stringify(reveal));
      });
    })
    .then(done, stopped);
})();
