// Puts two of this Mac's sessions and two of a connected device's on the bar, from what the board
// and your devices (PREVIEW_DEVICES) have, for a screenshot: docked in the notch (preview.sh's
// notch view), it opens on the first of that device's; floating (the bar view), the pills show.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_SIZE=1512,720 PREVIEW_INIT=scripts/preview/notch-devices-init.js scripts/preview.sh notch
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_INIT=scripts/preview/notch-devices-init.js scripts/preview.sh bar
(function () {
  var invoke = function (cmd) { return window.__TAURI_INTERNALS__.invoke(cmd, {}); };
  var pick = function (lanes, n, ok) {
    var out = [];
    lanes.forEach(function (l) {
      l.sessions.forEach(function (s) { if (out.length < n && !s.visitor && !s.archived && ok(l)) out.push({ lane: l, session: s }); });
    });
    return out;
  };
  var item = function (at, group, n) {
    return { key: at.session.key, group: group, status: group, waiting: false, asks: false, title: at.session.title, account: at.session.account, laneId: at.lane.id, callsign: at.lane.callsign, project: at.lane.project, hidden: false, detail: null, since: 1000 + n, openTarget: '', target: null, snoozedUntil: null, device: at.lane.device };
  };
  var t = setInterval(function () {
    var shape = document.querySelector('.ca-notch-shape');
    var docked = shape && document.querySelector('.ca-notch-count');
    if (!docked && !document.querySelector('.ca-hud-pill')) return;
    clearInterval(t);
    Promise.all([invoke('bar_feed'), invoke('get_board'), invoke('devices')]).then(function (all) {
      var here = pick(all[1].lanes, 2, function () { return true; });
      var there = pick(all[2].lanes, 2, function (l) { return l.device.online; });
      if (there.length === 0) return;
      var items = [item(here[0], 'needs', 0), item(there[0], 'unread', 1)].concat(here[1] ? [item(here[1], 'unread', 2)] : []).concat(there[1] ? [item(there[1], 'working', 3)] : []);
      window.__emit('bar-feed', Object.assign({}, all[0], { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [], devices: [there[0].lane.device.short] }));
      if (!docked) return;
      shape.click();
      var p = setInterval(function () {
        var pill = document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + there[0].session.key + '"]');
        if (!pill) return;
        clearInterval(p);
        pill.click();
      }, 60);
    });
  }, 60);
})();
