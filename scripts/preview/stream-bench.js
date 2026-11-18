// Times a reply streaming into an open conversation (SessionThread.tsx `Writing`): a session with
// a long conversation open in its panel, then 500 chunks of a Markdown reply, one every 10 ms as
// a hosted session's agent sends them (acp.rs hands over the whole reply so far each time), then
// 20 board pushes that change another session while it's open. Run on the real clock:
//   PREVIEW_WAIT=120000 scripts/preview.sh sessions '' scripts/preview/stream-bench.js
// The title ends up as "done: stream: …; pushes: …": how long the page's own work took while it
// streamed (script, and the layout it asked for), how many commits it made, how late its own
// timers came in all, the longest gap between frames, how long the last chunk took to show, and how many rows of the
// conversation rendered again; for the pushes, the app's own push commit times (perf.ts).
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var CHUNKS = 500;
  var EVERY = 10;
  var now = Date.now();
  var picked = null;

  // Commits, and renders of the conversation's rows, from React's devtools hook, which this
  // script sets before the app's module script runs. A row rendered when the element it draws
  // first (something said, a call's line, a run of calls) holds props it didn't at the last
  // commit: the build's component names are minified, its class names aren't.
  var commits = 0;
  var rowRenders = 0;
  var seen = new WeakMap();
  var ROW = /^ca-thread-(said|tool|run)( |$)/;
  var countRows = function (top) {
    var stack = [top];
    while (stack.length) {
      var f = stack.pop();
      var p = f.memoizedProps;
      if (typeof f.type === 'string' && p && typeof p.className === 'string' && ROW.test(p.className)) {
        // A fiber and its alternate: a render is new props on either.
        var alt = f.alternate;
        if (seen.get(f) !== p && !(alt && seen.get(alt) === p)) rowRenders++;
        seen.set(f, p);
        if (alt) seen.set(alt, p);
      }
      for (var c = f.child; c; c = c.sibling) stack.push(c);
    }
  };
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    supportsFiber: true,
    inject: function () { return 1; },
    onCommitFiberRoot: function (id, root) {
      commits++;
      try { countRows(root.current); } catch (e) { /* counted no rows */ }
    },
    onCommitFiberUnmount: function () {},
    onPostCommitFiberRoot: function () {},
    checkDCE: function () {},
  };
  // The page's own work: each task React's scheduler posts itself over a MessageChannel, each
  // frame callback, and each chunk handed over, timed to the end of the microtasks they queued
  // (React renders a store's change in one): script, and the layout it asks for as it goes.
  var work = 0;
  var timed = function (f) {
    return function () {
      var t = performance.now();
      try {
        return f.apply(this, arguments);
      } finally {
        queueMicrotask(function () { work += performance.now() - t; });
      }
    };
  };
  var Channel = window.MessageChannel;
  window.MessageChannel = function () {
    var ch = new Channel();
    var set = Object.getOwnPropertyDescriptor(MessagePort.prototype, 'onmessage').set;
    var handler = null;
    Object.defineProperty(ch.port1, 'onmessage', {
      configurable: true,
      get: function () { return handler; },
      set: function (h) { handler = h; set.call(ch.port1, h && timed(h)); },
    });
    return ch;
  };
  var frameNow = window.requestAnimationFrame;
  window.requestAnimationFrame = function (f) { return frameNow.call(window, timed(f)); };

  // A long conversation already there: prompts, replies with Markdown, and calls with results.
  var md = function (i) {
    return '## Step ' + i + '\n\nThe **importer** reads each row of `rows.csv`, checks it and writes it back:\n\n- one\n- two with `code`\n- three\n\n```ts\nconst x = read(' + i + ');\nwrite(x);\n```\n\nAnd a [link](https://example.com/' + i + ') to finish.';
  };
  var items = [];
  var cursor = 1;
  for (var i = 0; i < 40; i++) {
    items.push({ at: now - (60 - i) * 60e3, kind: 'you', text: 'Prompt ' + i + ': tidy up the northwind importer, part ' + i + '.', failed: false, cursor: cursor++, n: 0 });
    for (var j = 0; j < 3; j++) {
      var id = 'call-' + i + '-' + j;
      items.push({ at: now - (60 - i) * 60e3, kind: 'tool', id: id, text: 'Read · src/importer/part' + j + '.ts', input: 'src/importer/part' + j + '.ts', failed: false, cursor: cursor++, n: 0 });
      items.push({ at: now - (60 - i) * 60e3, kind: 'result', id: id, text: '', output: 'line one\nline two', failed: false, cursor: cursor++, n: 0 });
    }
    items.push({ at: now - (60 - i) * 60e3, kind: 'agent', text: md(i), failed: false, cursor: cursor++, n: 0 });
  }
  var page = { items: items, before: null, after: cursor };
  // The reply, chunk by chunk.
  var reply = '';
  var source = '';
  for (var k = 0; source.length < CHUNKS * 25; k++) source += md(100 + k) + '\n\n';
  var chunkAt = function (n) { return source.slice(0, Math.round((n * source.length) / CHUNKS)); };

  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_page' && mine) return Promise.resolve(JSON.parse(JSON.stringify(page)));
    if (cmd === 'session_newer' && mine) return Promise.resolve({ items: [], before: null, after: (args || {}).after || cursor });
    if (cmd === 'session_stream' && mine) return Promise.resolve(null);
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
          s.hosted = true;
          s.title = 'Stream bench';
          if (!picked) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var stats = function (xs) {
    xs = xs.slice().sort(function (a, b) { return a - b; });
    var at = function (f) { return xs[Math.min(xs.length - 1, Math.floor(f * xs.length))]; };
    return 'median ' + at(0.5) + ' ms, p90 ' + at(0.9) + ' ms, max ' + xs[xs.length - 1] + ' ms';
  };
  var result = '';

  say('stream bench: waiting for the list');
  until(function () { return picked && q('#row-' + CSS.escape(picked.lane + '::' + picked.key)); }, 20000)
    .then(function () {
      q('#row-' + CSS.escape(picked.lane + '::' + picked.key)).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return thread() && /Prompt 39/.test(thread().textContent); }, 8000);
    })
    .then(function () { return wait(1000); })
    .then(function () {
      say('stream bench: streaming');
      var c0 = commits;
      var r0 = rowRenders;
      var w0 = work;
      var late = 0;
      var frameGap = 0;
      var lastFrame = performance.now();
      var framing = true;
      (function frame() {
        var t = performance.now();
        frameGap = Math.max(frameGap, t - lastFrame);
        lastFrame = t;
        if (framing) requestAnimationFrame(frame);
      })();
      var start = performance.now();
      return new Promise(function (resolve) {
        var n = 0;
        var due = performance.now();
        (function next() {
          late += Math.max(0, performance.now() - due);
          n++;
          reply = chunkAt(n);
          timed(function () { window.__emit('hosted-stream', { key: picked.key, text: reply, tool: null }); })();
          if (n >= CHUNKS) return resolve();
          due += EVERY;
          setTimeout(next, Math.max(0, due - performance.now()));
        })();
      }).then(function () {
        var sent = performance.now();
        var tail = reply.slice(-40).replace(/[`*#\-\[\]()]/g, '').trim().split(/\s+/).pop();
        return until(function () {
          var last = [].slice.call(thread().querySelectorAll('.ca-thread-said.is-agent')).pop();
          return last && last.textContent.indexOf(tail) >= 0 && /Step 1\d\d/.test(last.textContent);
        }, 20000).then(function () {
          var shown = Math.round(performance.now() - sent);
          var wall = Math.round(performance.now() - start);
          framing = false;
          result =
            'stream: ' + Math.round(work - w0) + ' ms of work over ' + wall + ' ms (ideal ' + CHUNKS * EVERY + '), ' + (commits - c0) + ' commits for ' + CHUNKS + ' chunks, timers late ' + Math.round(late) + ' ms in all, longest frame gap ' + Math.round(frameGap) + ' ms, last chunk shown after ' + shown + ' ms, ' +
            (rowRenders - r0) + ' row renders, ' + reply.length + ' chars';
        });
      });
    })
    .then(function () {
      say('stream bench: pushes');
      return fetch('./board.json').then(function (r) { return r.text(); });
    })
    .then(function (text) {
      var times = [];
      var r0 = rowRenders;
      var marks = function () { return window.__calls.filter(function (c) { return c[0] === 'perf_mark' && /^push:/.test(c[1].line); }); };
      var round = 0;
      var next = function () {
        if (round >= 21) return Promise.resolve();
        var b = JSON.parse(text);
        b.perfLog = true;
        b.generatedAt = Date.now();
        var others = b.lanes.filter(function (l) { return l.sessions.length > 0 && l.id !== picked.lane; });
        var lane = others[round % others.length];
        lane.lastActivity += round + 1;
        lane.sessions[0].lastActivity += round + 1;
        // The open session as the bench made it.
        b.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) {
            if (s.key !== picked.key) return;
            s.status = 'working';
            s.statusSince = now - 60e3;
            s.lastActivity = now - 60e3;
            s.stalledSince = undefined;
            s.background = undefined;
            s.hosted = true;
            s.title = 'Stream bench';
          });
        });
        var before = marks().length;
        round++;
        window.__emit('board', b);
        return until(function () { return marks().length > before; }, 10000)
          .then(function () {
            var m = /commit (\d+) ms/.exec(marks()[before][1].line);
            if (round > 1) times.push(Number(m[1]));
            return wait(50);
          })
          .then(next);
      };
      return next().then(function () {
        result += '; pushes: commit ' + stats(times) + ' over ' + times.length + ', ' + (rowRenders - r0) + ' row renders';
      });
    })
    .then(function () { say('done: ' + result); })
    .catch(function (e) { say('done: stream bench failed: ' + (e && e.message) + ' ' + result); });
})();
