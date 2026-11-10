// Shows a listening session's cue in the bar, as its feed brings it (barfeed.rs `listenings`), for
// a screenshot: the first session on the bar listens, a coach with something to say, under the
// notch's pills (preview.sh's notch view), the notch's mark with its center red while it hears.
//   PREVIEW_SIZE=1512,420 PREVIEW_NAME=voice-cue PREVIEW_INIT=scripts/preview/voice-cue-init.js scripts/preview.sh notch
(function () {
  window.__PREVIEW_VOICE__ = true;
  var t = setInterval(function () {
    if (!document.querySelector('.ca-notch-shape, .ca-hud')) return;
    clearInterval(t);
    fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (feed) {
      var first = (feed.items || []).concat(feed.recent || [])[0];
      if (!first) return;
      window.__LISTENS__ = first.key;
      return window.__TAURI_INTERNALS__.invoke('bar_feed').then(function (now) { window.__emit('bar-feed', now); });
    });
  }, 60);
})();
