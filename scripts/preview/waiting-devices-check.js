// Checks another device's session in the waiting window (Waiting.tsx): one of a connected
// device's that needs you waits there with this Mac's, the longest wait first, and shows with its
// header saying the device; with `[devices] in_bar` off it leaves, and this Mac's whose turn ended
// takes its place. No PR waits on your review here.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-devices-check.js
(function () {
  var now = Date.now();
  var mine = null;
  var theirs = null;
  // One session here and one on a connected device wait on you, theirs the longer; nothing else.
  var real = window.__TAURI_INTERNALS__.invoke;
  var settle = function (lanes, ok, pick) {
    var chosen = null;
    lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        s.snoozedUntil = undefined;
        s.raised = undefined;
        if (!chosen && ok(l) && !l.hidden && !s.visitor && !s.archived) chosen = s.key;
        if (s.key === (pick() || chosen)) {
          s.status = l.device ? 'needs' : 'unread';
          s.statusSince = now - (l.device ? 3 : 1) * 3600e3;
          s.asks = !!l.device;
        } else if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
      });
    });
    return chosen;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'get_board') {
      return p.then(function (b) {
        // No PR waits on your review, which would come before a finished turn.
        b.prs = [];
        mine = mine || settle(b.lanes, function () { return true; }, function () { return mine; });
        if (mine) settle(b.lanes, function () { return true; }, function () { return mine; });
        return b;
      });
    }
    if (cmd === 'devices') {
      return p.then(function (d) {
        theirs = theirs || settle(d.lanes, function (l) { return l.device.online; }, function () { return theirs; });
        if (theirs) settle(d.lanes, function (l) { return l.device.online; }, function () { return theirs; });
        return d;
      });
    }
    return p;
  };
  var badge = function () { var b = document.querySelector('.ca-waiting .ca-devbadge'); return b ? b.textContent.trim() : ''; };
  var shown = function () { return document.querySelector('.ca-waiting .ca-sessinsp-head, .ca-waiting .ca-sessinsp-title'); };

  say('waiting devices check: waiting for it');
  until(function () { return !!shown() && !!theirs; }, 15000, 'a session shown')
    .then(function () { return until(function () { return badge() !== ''; }, 3000, 'the device’s session').catch(function () {}); })
    .then(function () {
      check('another device’s session waits here, the longest wait first', badge() === 'On Mac mini', badge() || 'no badge');
      return real('get_board', {});
    })
    .then(function (b) {
      b.devices = Object.assign({}, b.devices, { in_bar: false });
      b.prs = [];
      mine && settle(b.lanes, function () { return true; }, function () { return mine; });
      window.__emit('board', b);
      return until(function () { return !!shown() && badge() === ''; }, 3000, 'this Mac’s').catch(function () {});
    })
    .then(function () {
      check('turned off, it leaves and this Mac’s shows', !!shown() && badge() === '', badge());
      done();
    })
    .catch(stopped);
})();
