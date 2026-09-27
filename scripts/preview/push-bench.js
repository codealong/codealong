// Times board pushes as the engine sends them: a fresh board each time (as IPC hands one over)
// with one worktree and one of its sessions changed, which is what most publishes carry.
// Run on the real clock: PREVIEW_WAIT=60000 scripts/preview.sh <view> '' scripts/preview/push-bench.js
// The title ends up as "done: push commit: median … ms, p90 … ms, max … ms over N pushes".
(function () {
  var ROUNDS = 40;
  var times = [];
  var show = function (text) {
    window.__benchTitle = text;
  };
  // The app rewrites the title as counts change, so ours is put back until Chrome reads it.
  window.setInterval(function () {
    if (window.__benchTitle) document.title = window.__benchTitle;
  }, 100);
  var marks = function () {
    return window.__calls.filter(function (c) {
      return c[0] === 'perf_mark' && /^push:/.test(c[1].line);
    });
  };
  var wait = function (ms) {
    return new Promise(function (r) {
      window.setTimeout(r, ms);
    });
  };
  var until = function (test, ms) {
    var start = Date.now();
    return new Promise(function (resolve, reject) {
      (function poll() {
        if (test()) return resolve();
        if (Date.now() - start > ms) return reject(new Error('timed out'));
        window.setTimeout(poll, 5);
      })();
    });
  };
  show('push bench: loading');
  fetch('./board.json')
    .then(function (r) {
      return r.text();
    })
    .then(function (text) {
      return until(function () {
        return document.querySelectorAll('[role="option"]').length > 0;
      }, 20000).then(function () {
        return text;
      });
    })
    .then(function (text) {
      var round = 0;
      var next = function () {
        if (round >= ROUNDS) return Promise.resolve();
        var b = JSON.parse(text);
        b.perfLog = true;
        b.generatedAt = Date.now();
        var withSessions = b.lanes.filter(function (l) {
          return l.sessions.length > 0;
        });
        var lane = withSessions[round % withSessions.length] || b.lanes[round % b.lanes.length];
        lane.lastActivity += round + 1;
        if (lane.sessions[0]) lane.sessions[0].lastActivity += round + 1;
        var before = marks().length;
        round++;
        show('push bench: push ' + round + ', ' + marks().length + ' marks');
        window.__emit('board', b);
        return until(function () {
          return marks().length > before;
        }, 10000)
          .then(function () {
            var m = /commit (\d+) ms/.exec(marks()[before][1].line);
            // The first push after load warms things up; it isn't counted.
            if (round > 1) times.push(Number(m[1]));
            return wait(50);
          })
          .then(next);
      };
      return next();
    })
    .then(function () {
      times.sort(function (a, b) {
        return a - b;
      });
      var at = function (q) {
        return times[Math.min(times.length - 1, Math.floor(q * times.length))];
      };
      show('done: push commit: median ' + at(0.5) + ' ms, p90 ' + at(0.9) + ' ms, max ' + times[times.length - 1] + ' ms over ' + times.length + ' pushes');
    })
    .catch(function (e) {
      show('done: push bench failed: ' + e.message);
    });
})();
