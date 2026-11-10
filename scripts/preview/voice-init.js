// Turns voice on and has a session listen (the first listed that isn't shown yet, so its
// conversation is read as a listening one's), shown in its panel, for a screenshot of
// a listening session (preview.sh's sessions view): a coach a few minutes into a call, its
// conversation the meeting it heard, the strip over its box with what it hasn't been told yet and
// the words being said.
//   PREVIEW_INIT=scripts/preview/voice-init.js scripts/preview.sh sessions
(function () {
  window.__PREVIEW_VOICE__ = true;
  var t = setInterval(function () {
    var rows = Array.prototype.slice.call(document.querySelectorAll('.ca-board-list [id^="row-"][id*="::"]'));
    var row = rows.filter(function (r) { return r.getAttribute('aria-selected') !== 'true'; })[0];
    if (!row || !document.querySelector('[data-shows-session]')) return;
    clearInterval(t);
    var key = row.id.slice(row.id.indexOf('::') + 2);
    window.__LISTENS__ = key;
    window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
      window.__emit('board', board);
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      // Once its strip has read where it stands, the words being said now.
      var said = setInterval(function () {
        if (!document.querySelector('.ca-live-lines')) return;
        clearInterval(said);
        window.__emit('hearing', { session: key, who: 'Them', text: 'and the other thing we’d need is single sign' });
      }, 60);
    });
  }, 60);
})();
