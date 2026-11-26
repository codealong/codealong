// Checks a session stopped at its usage limit on an account out of credits too (its extra usage
// refused: `SessionLimit.spendCapped`), in the bar's peek: beside its Limit, Snooze until it has
// room snoozes it until the account has room again (snooze.ts `suggestedSnooze`, `room`), the
// limit's lift the latest, as its Snooze menu's first line, Until it has room, and ⌃⌘8 do. Once
// the engine has it snoozed so (`Session.snoozedForRoom`), its header says until it has room in
// place of a time. limit-check.js has a limit that only lifts. The limit is put on a real Claude
// Code session, since a board rarely has one.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/limit-room-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : null;
  };

  var board = get('./board.json');
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && !l.hidden && !l.device && !s.archived && s.harness === 'claude' && s.resumeCommand) picked = s;
    });
  });
  if (!picked) { stopped('no Claude Code session to stop at a limit'); return; }
  var sk = picked.key;
  var lifts = Date.now() + 26 * 3600 * 1000;
  localStorage.setItem('codealong.preview.peek', sk);
  var limited = function (s, snoozed) {
    s.status = 'error';
    s.excerpt = 'You’ve hit your weekly limit';
    s.limit = { meter: 'week', resetsAt: lifts, spendCapped: true, at: Date.now() - 60000, moveTo: null, continues: true };
    if (snoozed) {
      s.snoozedUntil = lifts;
      s.snoozedForRoom = true;
    }
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === sk) limited(s, false); }); });
      return b;
    });
  };
  var banner = function () { return q('.ca-sessinsp-head .ca-sessinsp-limit'); };
  var button = function () { var l = banner(); return l && all('button', l).filter(function (b) { return /^Snooze until /.test(b.textContent); })[0]; };

  (async function () {
    try {
      var snooze = await until(button, 20000, 'the limit banner’s snooze');
      check('it offers a snooze until its account has room', snooze.textContent === 'Snooze until it has room', snooze.textContent);
      check('which says what frees it, the lift the latest, and its key', /^It leaves what waits on you until its account has room again: once credits are bought, its spend limit is raised or a reset is used, and when the limit lifts, .*, at the latest \((⌃⌘|Ctrl\+Alt\+)8\)$/.test(snooze.title), snooze.title);
      var more = q('.ca-sessinsp-head [aria-label="More"]');
      if (more) {
        var n = menus();
        more.click();
        await until(function () { return menus() > n; }, 3000, 'its menu');
        var labels = menuLabels();
        check('its Snooze menu offers it first', labels.indexOf('Snooze › Until it has room') >= 0 || labels.indexOf('Until it has room') >= 0, labels.join(' | '));
        key(document.body, { key: 'Escape' });
      } else {
        note('no ⋯ button in the peek to read its menu from');
      }
      snooze.click();
      await until(function () { return calls('snooze_sessions').length > 0; }, 3000, 'the snooze');
      var asked = calls('snooze_sessions')[0][1];
      check('it snoozes until room, the lift the latest', asked.keys.join() === sk && asked.until === lifts && asked.room === true, JSON.stringify(asked));

      // The engine has it snoozed until room: its header says so in place of a time.
      var b = JSON.parse(JSON.stringify(board));
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === sk) limited(s, true); }); });
      window.__emit('board', b);
      var meta = await until(function () { var m = q('.ca-sessinsp-head .ca-sessinsp-meta'); return m && /snoozed until/.test(m.textContent) && m; }, 5000, 'its header’s snooze');
      check('its header says it’s snoozed until it has room', /snoozed until it has room/.test(meta.textContent), meta.textContent);
      check('and the banner no longer offers the snooze', !button(), button() ? button().textContent : '');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
