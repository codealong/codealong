// Checks a conversation on another Mac as that Mac drops out and comes back (SessionThread.tsx
// `readAgain`): read while the Mac is away, it says why in its place, and isn't asked for again
// until the Mac is back and heard from, when it shows; read again once it's shown, a read that
// fails keeps it on screen and says so in a notice, which goes once a read works; and a read that
// fails with the Mac there all along is asked for again only once, not with every feed.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/remote-thread-check.js
(function () {
  // This runs before the app's script: the first session on the Mac mini is in Your turn.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var MINI = '9f8e7d6c5b';
  var AWAY = 'Mac mini isn’t connected; it was last seen just now.';
  var SAID = 'Fix the login redirect';
  var picked = null;
  var view = null;
  // Whether a read of its conversation fails, as one asked while the Mac mini is away does.
  var fails = true;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && args && args.key === picked.key) {
      window.__calls.push([cmd, args, options]);
      if (fails) return Promise.reject(AWAY);
      var at = Date.now() - 60e3;
      return Promise.resolve({ before: null, after: 2, items: [
        { at: at, kind: 'you', text: SAID, failed: false, cursor: 1, n: 0 },
        { at: at + 30e3, kind: 'agent', text: 'Fixed: the redirect keeps the path.', failed: false, cursor: 2, n: 0 },
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'devices') return p;
    return p.then(function (v) {
      (v.lanes || []).forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || l.device.short !== MINI || s.visitor) return;
          s.status = 'unread';
          s.statusSince = Date.now() - 10 * 60e3;
          s.lastActivity = s.statusSince;
          s.archived = false;
          s.asks = false;
          s.snoozedUntil = undefined;
          s.title = 'Far A';
          picked = { key: s.key, lane: l.id };
        });
      });
      view = v;
      return v;
    });
  };

  /** The Mac mini as its feed would have it now: there or away, heard from just now when it's
   *  there, its session going on from another's record (`continues`) when one is given. */
  var mini = function (online, continues) {
    var v = JSON.parse(JSON.stringify(view));
    var seen = online ? Date.now() : Date.now() - 30e3;
    v.devices.forEach(function (d) { if (d.short === MINI) Object.assign(d, { online: online, lastSeen: seen }); });
    v.lanes.forEach(function (l) {
      if (l.device.short !== MINI) return;
      Object.assign(l.device, { online: online, lastSeen: seen });
      l.sessions.forEach(function (s) { if (s.key === picked.key && continues) s.continues = continues; });
    });
    view = v;
    window.__emit('devices', v);
    return wait(400);
  };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var shown = function () { var t = thread(); return !!t && !t.classList.contains('is-fallback') && t.textContent.indexOf(SAID) >= 0; };
  var failedView = function () { var t = thread(); return t && t.classList.contains('is-fallback') ? t.textContent : ''; };
  var reads = function () { return calls('session_page').filter(function (c) { return c[1].key === picked.key; }).length; };
  var notice = function () { var n = q('.ca-notice'); return n ? n.textContent : ''; };

  say('remote thread check: waiting for the Mac mini’s session');
  until(function () { return !!row() && !!view; }, 20000, 'no session on the Mac mini (run with PREVIEW_DEVICES)')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return failedView().indexOf(AWAY) >= 0; }, 5000, 'the read while it’s away failing');
    })
    .then(function () {
      check('read while its Mac is away, it says why in the conversation’s place', true);
      var before = reads();
      return mini(false).then(function () {
        check('it isn’t asked for again while the Mac is away', reads() === before, reads() + ' reads, ' + before + ' before');
        fails = false;
        return mini(true);
      });
    })
    .then(function () { return until(shown, 5000, 'the conversation once the Mac mini is back'); })
    .then(function () {
      check('once the Mac is back and heard from, it’s read again and shows', shown());
      fails = true;
      var before = reads();
      // A job going on from another session's record has the conversation read from the start.
      return mini(true, '@' + MINI + ':claude:claude:from-one').then(function () {
        return until(function () { return reads() > before; }, 3000, 'the conversation read again');
      });
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('a read that fails keeps what was shown', shown(), failedView() || (thread() ? thread().textContent.slice(0, 120) : 'no thread'));
      check('and says so in a notice', notice().indexOf('A conversation isn’t updating') >= 0, notice() || 'no notice');
      fails = false;
      var before = reads();
      return mini(false).then(function () { return mini(true); }).then(function () {
        return until(function () { return reads() > before && notice() === ''; }, 5000, 'a read once it’s back, and the notice going');
      });
    })
    .then(function () {
      check('once the Mac is back it’s read again, and the notice goes', shown() && notice() === '', notice());
      // A record that won't read with the Mac there: asked for again once, not with every feed.
      fails = true;
      var before = reads();
      return mini(true, '@' + MINI + ':claude:claude:from-two')
        .then(function () { return until(function () { return reads() === before + 1; }, 3000, 'the read that fails'); })
        .then(function () { return mini(true); })
        .then(function () { return mini(true); })
        .then(function () { return mini(true); })
        .then(function () {
          check('one that fails with the Mac there is asked for again only once', reads() === before + 2, reads() - before + ' reads');
          check('what was shown stays through it', shown());
        });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
