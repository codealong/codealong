// Checks a conversation following its end as a reply streams in (SessionThread.tsx `onScroll`,
// `follows`): it opens at the end and stays there as batches grow the reply. Scrolled up by a few
// pixels, well inside the margin that counts as near the end, more coming in leaves it where you
// put it, batch after batch, and so does a scroll up made just before a batch, before the view has
// heard it. Back at the end, it follows again; the reply growing shorter there, which brings the
// end up to meet the view, isn't a scroll up, and it goes on following. Scrolled well up, nothing
// moves it. Runs on the real clock, since scroll events come with frames:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/thread-follow-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var now = Date.now();
  var picked = null;
  var paras = function (what, n) {
    var lines = [];
    for (var i = 1; i <= n; i++) lines.push(what + ', paragraph ' + i + ': the northwind importer reads each row, checks it and writes it back.');
    return lines.join('\n\n');
  };
  var first = {
    items: [
      { at: now - 30 * 60e3, kind: 'you', text: 'Tidy up the northwind importer.', failed: false, cursor: 1, n: 0 },
      { at: now - 29 * 60e3, kind: 'agent', text: paras('Earlier', 30), failed: false, cursor: 2, n: 0 },
      { at: now - 20 * 60e3, kind: 'you', text: 'Now the atlas exporter.', failed: false, cursor: 3, n: 0 },
    ],
    before: null,
    after: 3,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_page' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(JSON.parse(JSON.stringify(first)));
    }
    if (cmd === 'session_newer' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ items: [], before: null, after: (args || {}).after || 3 });
    }
    if (cmd === 'session_stream' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(window.__stream || null);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'working') s.status = 'idle';
          if (picked ? s.key !== picked.key : l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          s.status = 'working';
          s.statusSince = now - 60e3;
          s.lastActivity = now - 60e3;
          s.stalledSince = undefined;
          s.background = undefined;
          s.hosted = false;
          s.title = 'Follow the end';
          if (!picked) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var gap = function () { var el = thread(); return el.scrollHeight - el.scrollTop - el.clientHeight; };
  // Two frames: the scroll a change made is heard, and what a batch laid out is placed.
  var frames = function () {
    return new Promise(function (r) { requestAnimationFrame(function () { requestAnimationFrame(r); }); }).then(function () { return wait(60); });
  };
  var n = 0;
  var stream = function (paragraphs) {
    n = paragraphs;
    window.__stream = { key: picked.key, text: paras('Writing', paragraphs), tool: null };
    window.__emit('hosted-stream', window.__stream);
  };
  // A batch that grows the reply, and the frames for it to be laid out and placed.
  var more = function () { stream(n + 2); return frames(); };
  var up = function (px) { thread().scrollTop -= px; return thread().scrollTop; };
  var stays = function (name, top) { check(name, Math.abs(thread().scrollTop - top) < 1, 'at ' + thread().scrollTop + ', left at ' + top + ', ' + gap() + ' from the end'); };

  say('thread follow check: waiting for the list');
  until(function () { return picked && q('#row-' + CSS.escape(picked.lane + '::' + picked.key)); }, 20000)
    .then(function () {
      q('#row-' + CSS.escape(picked.lane + '::' + picked.key)).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return thread() && /Now the atlas exporter/.test(thread().textContent); }, 8000);
    })
    .then(frames)
    .then(function () {
      stream(4);
      return until(function () { return /Writing, paragraph 4/.test(thread().textContent); }, 3000).then(frames);
    })
    .then(function () {
      check('it opens at the end', gap() < 2, String(gap()));
      return more();
    })
    .then(function () {
      check('a batch keeps the end in view', gap() < 2, String(gap()));
      var top = up(8);
      return frames()
        .then(more)
        .then(function () { stays('a few pixels up, a batch leaves it where it was', top); })
        .then(more)
        .then(more)
        .then(function () { stays('and so do the batches after', top); });
    })
    .then(function () {
      thread().scrollTop = thread().scrollHeight;
      return frames().then(more);
    })
    .then(function () {
      check('back at the end, it follows again', gap() < 2, String(gap()));
      // Up, and a batch in the same moment, before the scroll's been heard.
      var top = up(8);
      stream(n + 2);
      return frames().then(function () { stays('a scroll up just before a batch holds', top); });
    })
    .then(function () {
      thread().scrollTop = thread().scrollHeight;
      return frames();
    })
    .then(function () {
      // Shorter: the end comes up to meet the view, which is still at the end.
      stream(n - 6);
      return frames();
    })
    .then(more)
    .then(function () {
      check('the reply growing shorter at the end isn’t a scroll up', gap() < 2, String(gap()));
      var top = up(400);
      return frames()
        .then(more)
        .then(more)
        .then(function () { stays('scrolled well up, nothing moves it', top); });
    })
    .then(done)
    .catch(function (e) {
      check('no error', false, String((e && e.stack) || e));
      done();
    });
})();
