// preview.sh cardpop: the popover of a card pinned to the desktop (DesktopPopover.tsx), opened as
// cardpop.rs opens it, for PREVIEW_PIN's card, on the side PREVIEW_SIDE says (left by default: new
// cards stack in the top right corner, where there's no room on their right).
window.__CODEALONG_CARDPOP__ = true;
window.addEventListener('load', function () {
  setTimeout(function () {
    window.__TAURI_INTERNALS__.invoke('desktop_feed').then(function (feed) {
      window.__emit('card-popover', { id: 1, feed: feed, side: window.__CARDPOP_SIDE__ || 'left', room: 860 });
    });
  }, 400);
});
