// Has a meeting counted down to before its offer (meetings.rs `counting_down`), for a screenshot
// of its countdown wherever it shows: beside the notch's camera, closed; a pill ahead of the
// floating bar's pills; the minutes on the main window's Meetings button.
//   PREVIEW_NAME=voice-meeting-soon PREVIEW_INIT=scripts/preview/voice-meeting-soon-init.js scripts/preview.sh notch
//   PREVIEW_NAME=voice-meeting-soon-bar PREVIEW_INIT=scripts/preview/voice-meeting-soon-init.js scripts/preview.sh bar
//   PREVIEW_NAME=voice-meeting-soon-main PREVIEW_INIT=scripts/preview/voice-meeting-soon-init.js scripts/preview.sh sessions
(function () {
  window.__PREVIEW_VOICE__ = true;
  window.__MEETING_SOON__ = { id: 'E2@1794330000000', title: 'Design review', start: Date.now() + 4 * 60e3 - 20e3, end: Date.now() + 34 * 60e3, calendar: 'Work', attendees: ['Dana Kestrel'] };
})();
