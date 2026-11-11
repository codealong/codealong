// Has a meeting on offer (meetings.rs `Offer`), for a screenshot of it wherever it shows: the
// card under the notch's pills, the line ahead of the floating bar's pills, the line beside the
// main window's notices. With PREVIEW_MEETINGS=1 in the page's address it also opens the main
// window's Meetings dialog, on the stub's two kept meetings.
//   PREVIEW_SIZE=1512,420 PREVIEW_NAME=voice-meeting PREVIEW_INIT=scripts/preview/voice-meeting-init.js scripts/preview.sh notch
//   PREVIEW_NAME=voice-meeting-main PREVIEW_INIT=scripts/preview/voice-meeting-init.js scripts/preview.sh sessions
(function () {
  window.__PREVIEW_VOICE__ = true;
  window.__MEETING__ = { id: 'E1@1794330000000', title: 'Renewal call', start: Date.now() + 55e3, end: Date.now() + 31 * 60e3, calendar: 'Work', attendees: ['Dana Kestrel', 'Lee Atlas'] };
  if (!window.__OPEN_MEETINGS__) return;
  var t = setInterval(function () {
    var opens = document.querySelector('.ca-win-bar button[aria-label="Meetings"]');
    if (!opens) return;
    clearInterval(t);
    opens.click();
  }, 60);
})();
