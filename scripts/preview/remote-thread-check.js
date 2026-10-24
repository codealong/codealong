// Checks a conversation on another Mac as that Mac drops out and comes back (SessionThread.tsx
// `readAgain`): read while the Mac is away, it says why in its place, and isn't asked for again
// until the Mac is back, when it shows, though nothing new came from the Mac; read again once
// it's shown, a read that fails keeps it on screen, says so in a notice, and doesn't page the
// record back from the old read's place; once the Mac is back it's read again and the notice
// goes; a read whose Mac came back before it failed is read again; a read that lands asks for
// what came while it was on its way; and one that fails with the Mac there all along is asked
// for again only once, not with every feed.
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
  // How a read of its conversation from the start goes: failing, as one asked while the Mac mini
  // is away does, and after how long. Its first page begins after some of the record (`before`),
  // and is long enough to scroll.
  var fails = true;
  var slow = 0;
  var page = function () {
    var at = Date.now() - 60e3;
    var items = [{ at: at, kind: 'you', text: SAID, failed: false, cursor: 11, n: 0 }];
    for (var i = 0; i < 40; i++) items.push({ at: at + i, kind: 'agent', text: 'Step ' + i + ' of the fix.\n\nIt keeps the path the redirect came from.', failed: false, cursor: 12 + i, n: 0 });
    return { before: 10, after: 60, items: items };
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && args && args.key === picked.key) {
      window.__calls.push([cmd, args, options]);
      // The pages before the first: one, the start of the record.
      if (args.before != null) return Promise.resolve({ before: null, after: 10, items: [] });
      var fail = fails;
      return new Promise(function (ok, no) { setTimeout(function () { return fail ? no(AWAY) : ok(page()); }, slow); });
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

  /** The Mac mini as its feed would have it now: there or away, and its session as `change`
   *  says (`continues`, another session's record it goes on from; `lastActivity`). There, it's
   *  heard from just now, unless `quiet`: a connection back with nothing new sends no feed. */
  var mini = function (online, change, quiet) {
    var v = JSON.parse(JSON.stringify(view));
    var tag = v.lanes.filter(function (l) { return l.device.short === MINI; })[0].device;
    var seen = online && !quiet ? Date.now() : tag.lastSeen != null && quiet ? tag.lastSeen : Date.now() - 30e3;
    v.devices.forEach(function (d) { if (d.short === MINI) Object.assign(d, { online: online, lastSeen: seen }); });
    v.lanes.forEach(function (l) {
      if (l.device.short !== MINI) return;
      Object.assign(l.device, { online: online, lastSeen: seen });
      l.sessions.forEach(function (s) { if (s.key === picked.key) Object.assign(s, change || {}); });
    });
    view = v;
    window.__emit('devices', v);
    return wait(400);
  };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var shown = function () { var t = thread(); return !!t && !t.classList.contains('is-fallback') && t.textContent.indexOf(SAID) >= 0; };
  var failedView = function () { var t = thread(); return t && t.classList.contains('is-fallback') ? t.textContent : ''; };
  var ofIt = function (cmd) { return calls(cmd).filter(function (c) { return c[1].key === picked.key; }); };
  var reads = function () { return ofIt('session_page').filter(function (c) { return c[1].before == null; }).length; };
  var notice = function () { var n = q('.ca-notice'); return n ? n.textContent : ''; };
  var readsSince = function (n, want, what) { return until(function () { return reads() >= n + want; }, 5000, what).then(function () { return wait(300); }); };

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
        return mini(true, null, true);
      });
    })
    .then(function () { return until(shown, 5000, 'the conversation once the Mac mini is back'); })
    .then(function () {
      check('once the Mac is back it’s read again and shows, though nothing new came from it', shown());
      fails = true;
      var n = reads();
      // A job going on from another session's record has the conversation read from the start.
      return mini(true, { continues: '@' + MINI + ':claude:claude:from-one' }).then(function () { return readsSince(n, 1, 'the conversation read again'); });
    })
    .then(function () {
      check('a read that fails keeps what was shown', shown(), failedView() || (thread() ? thread().textContent.slice(0, 120) : 'no thread'));
      check('and says so in a notice', notice().indexOf('A conversation isn’t updating') >= 0, notice() || 'no notice');
      var older = ofIt('session_page').filter(function (c) { return c[1].before != null; }).length;
      var t = thread();
      t.scrollTop = 0;
      t.dispatchEvent(new Event('scroll'));
      return wait(500).then(function () {
        var now = ofIt('session_page').filter(function (c) { return c[1].before != null; }).length;
        check('what it keeps isn’t paged back from the old read’s place', now === older, now - older + ' pages asked for');
      });
    })
    .then(function () {
      fails = false;
      var n = reads();
      return mini(false).then(function () { return mini(true); }).then(function () {
        return until(function () { return reads() > n && notice() === ''; }, 5000, 'a read once it’s back, and the notice going');
      });
    })
    .then(function () {
      check('once the Mac is back it’s read again, and the notice goes', shown() && notice() === '', notice());
      // The Mac comes back while a read that went with the old connection is still on its way.
      fails = true;
      slow = 1500;
      var n = reads();
      return mini(true, { continues: '@' + MINI + ':claude:claude:from-two' })
        .then(function () {
          fails = false;
          slow = 0;
          return mini(true);
        })
        .then(function () { return readsSince(n, 2, 'the read that failed after the Mac came back, read again'); })
        .then(function () { check('a read whose Mac came back before it failed is read again', shown() && notice() === '', notice()); });
    })
    .then(function () {
      // Activity while a read is on its way: once it lands, what's new since is asked for.
      slow = 2000;
      var n = reads();
      var newer;
      return mini(true, { continues: '@' + MINI + ':claude:claude:from-three' })
        .then(function () { return mini(true, { lastActivity: Date.now() }); })
        .then(function () { newer = ofIt('session_newer').length; })
        .then(function () { return until(function () { return ofIt('session_newer').length > newer; }, 5000, 'what’s new, once the read landed'); })
        .then(function () {
          check('a read that lands asks for what came while it was on its way', reads() === n + 1);
          slow = 0;
        });
    })
    .then(function () {
      // A record that won't read with the Mac there: asked for again once, not with every feed.
      fails = true;
      var n = reads();
      return mini(true, { continues: '@' + MINI + ':claude:claude:from-four' })
        .then(function () { return readsSince(n, 1, 'the read that fails'); })
        .then(function () { return mini(true); })
        .then(function () { return mini(true); })
        .then(function () { return mini(true); })
        .then(function () {
          check('one that fails with the Mac there is asked for again only once', reads() === n + 2, reads() - n + ' reads');
          check('what was shown stays through it', shown());
        });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
