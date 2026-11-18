// A crowded status bar, for window-chrome-check.js at the window's least width: voice on, with a
// meeting on offer whose name is long, beside the notices PREVIEW_NOTICES gives it.
//   PREVIEW_SIZE=980,700 PREVIEW_NOTICES=scripts/preview/notices-sample.json PREVIEW_INIT=scripts/preview/window-chrome-crowded-init.js PREVIEW_NAME=crowded scripts/preview.sh sessions
(function () {
  window.__PREVIEW_VOICE__ = true;
  window.__MEETING__ = { id: 'E2@1794330000000', title: 'Quarterly planning with the northwind and kestrel teams', start: Date.now() + 55e3, end: Date.now() + 31 * 60e3, calendar: 'Work', attendees: ['Dana Kestrel', 'Lee Atlas'] };
})();
