// Checks a session of another device's pinned to the desktop (DesktopPin.tsx, desktop.rs cutting it
// from that device's worktrees beside the board): its card shows it with its device's badge, faded
// while the device is away, a click shows it in the peek, a double-click, with nothing to open
// here, shows it in Codealong, and its menu takes it off the desktop.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 PREVIEW_PIN=session:@ scripts/preview.sh desktop '' scripts/preview/desktop-devices-check.js
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 PREVIEW_PIN=session:@4a3b2c1d0e scripts/preview.sh desktop '' scripts/preview/desktop-devices-check.js
(function () {
  var click = function (el, n) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: n })); };
  var card = function () { return document.querySelector('.ca-desk .ca-cv-card.is-session'); };
  var feed = null;
  var device = null;

  say('desktop devices check: waiting for the card');
  until(function () { return !!card(); }, 20000, 'the card')
    .then(function () { return Promise.all([window.__TAURI_INTERNALS__.invoke('desktop_feed'), window.__TAURI_INTERNALS__.invoke('devices')]); })
    .then(function (all) {
      feed = all[0];
      var lane = feed.lanes[0];
      device = lane && lane.device;
      check('it shows a session on another device', feed.pin.key.charAt(0) === '@' && !!device && lane.sessions[0].key === feed.pin.key, JSON.stringify(feed.pin));
      var badge = card().querySelector('.ca-devbadge');
      check('with its device’s badge', !!badge && badge.textContent.trim() === device.name.replace(/^[^\s’']+[’']s\s+/, ''), badge ? badge.textContent : 'none');
      check(device.online ? 'not faded while its device is connected' : 'faded while its device is away', card().classList.contains('is-away') === !device.online, card().className);
      click(document.querySelector('.ca-desk .ca-cv-l1'), 1);
      return wait(500);
    })
    .then(function () {
      var p = last('peek_open');
      check('a click shows it in the peek', !!p && p.key === feed.pin.key, JSON.stringify(p));
      window.__emit('peek', { key: feed.pin.key, engaged: true });
      return wait(100);
    })
    .then(function () {
      var shows = calls('show_main').length;
      click(document.querySelector('.ca-desk .ca-cv-l1'), 1);
      click(document.querySelector('.ca-desk .ca-cv-l1'), 2);
      var o = last('show_main');
      check('a double-click, with nothing to open here, shows it in Codealong', calls('open_from_bar').length === 0 && calls('show_main').length === shows + 1 && o.reveal.kind === 'session' && o.reveal.key === feed.pin.key, JSON.stringify(o));
      return menuOf(card());
    })
    .then(function (labels) {
      check('its menu takes it off the desktop', labels.indexOf('Remove from desktop') >= 0, labels.join(' | '));
      done();
    })
    .catch(stopped);
})();
