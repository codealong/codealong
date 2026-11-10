// Shows a key held to talk in the bar, as the engine tells it (dictation.rs `Talk`), for a
// screenshot: under the notch's pills (preview.sh's notch view), or as a line in the floating bar
// (its bar view). Words heard so far, for your orchestrator.
//   PREVIEW_SIZE=1512,420 PREVIEW_NAME=voice-talk PREVIEW_INIT=scripts/preview/voice-talk-init.js scripts/preview.sh notch
//   PREVIEW_NAME=voice-talk-bar PREVIEW_INIT=scripts/preview/voice-talk-init.js scripts/preview.sh bar
(function () {
  var t = setInterval(function () {
    if (!document.querySelector('.ca-notch-shape, .ca-hud')) return;
    clearInterval(t);
    window.__emit('talk', { id: 1, phase: 'listening', to: 'orchestrator', text: 'Check which sessions are waiting on me, and tell the release one to rebase onto', level: 0.2 });
  }, 60);
})();
