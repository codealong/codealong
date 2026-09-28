// Times the quick panel: opening it (the script quick.rs runs, with the board in it, until the
// page returns its height), typing into it (each key from its input event to the page drawing
// it), and board pushes while it's up. Run on the real clock:
//   PREVIEW_WAIT=120000 scripts/preview.sh quick '' scripts/preview/quick-bench.js
// The title ends up as "done: open …; key …; push …", each a median, p90 and max in ms, with how
// many times typing resized the window (`fits`) and how many long tasks it ran into.
(function () {
  var OPENS = 12;
  var PUSHES = 20;
  var QUERIES = ['codealong quick panel', 'fix the bar', 'perf'];
  var KEY_GAP_MS = 90;
  var show = function (text) {
    window.__benchTitle = text;
  };
  // The app rewrites the title as counts change, so ours is put back until Chrome reads it.
  window.setInterval(function () {
    if (window.__benchTitle) document.title = window.__benchTitle;
  }, 100);

  // Every command the page sends, with when it sent it. What sessions said is answered at once
  // from a board read once: the stub would read board.json again on the page's own thread,
  // where the app's engine answers from its index.
  var sent = [];
  var boardText = null;
  var inner = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    sent.push([cmd, performance.now()]);
    if (cmd === 'search_text' && boardText) {
      var b = JSON.parse(boardText);
      var all = b.lanes.flatMap(function (l) {
        return l.sessions;
      });
      return Promise.resolve({
        hits: all.slice(0, 2).map(function (s, i) {
          return { key: s.key, snippet: '…the ' + ((args || {}).words || []).join(' ') + ' part of it…', you: i === 1, at: s.lastActivity };
        }),
        building: false,
        off: false,
      });
    }
    return inner.call(this, cmd, args, options);
  };
  var count = function (cmd, since) {
    return sent.filter(function (c) {
      return c[0] === cmd && c[1] >= since;
    }).length;
  };
  var lastAt = function (cmd) {
    for (var i = sent.length - 1; i >= 0; i--) if (sent[i][0] === cmd) return sent[i][1];
    return null;
  };

  // Tasks of 50 ms or more, as Chrome reports them: what typing waits behind.
  var longTasks = [];
  try {
    new PerformanceObserver(function (list) {
      list.getEntries().forEach(function (e) {
        longTasks.push([e.startTime, e.duration]);
      });
    }).observe({ type: 'longtask', buffered: true });
  } catch (e) {}

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
        window.setTimeout(poll, 2);
      })();
    });
  };
  // After the next frame is drawn: a frame callback, then a task after it.
  var painted = function () {
    return new Promise(function (r) {
      requestAnimationFrame(function () {
        window.setTimeout(r, 0);
      });
    });
  };
  // After work already queued on the page's thread (React's render of a push) has run.
  var drained = function () {
    return new Promise(function (r) {
      var ch = new MessageChannel();
      ch.port1.onmessage = function () {
        r();
      };
      ch.port2.postMessage(null);
    });
  };
  var stats = function (xs) {
    var s = xs.slice().sort(function (a, b) {
      return a - b;
    });
    var at = function (q) {
      return s[Math.min(s.length - 1, Math.floor(q * s.length))];
    };
    return 'median ' + at(0.5).toFixed(0) + ', p90 ' + at(0.9).toFixed(0) + ', max ' + s[s.length - 1].toFixed(0);
  };

  var textarea = function () {
    return document.querySelector('.ca-quick-input textarea');
  };
  var setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
  // One key: the value as typing leaves it, then the input event React listens for.
  var type = function (el, value) {
    setValue.call(el, value);
    el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value.slice(-1) }));
  };

  var opens = [];
  var keysSync = [];
  var keysPaint = [];
  var pushes = [];
  var fits = 0;
  var typedFrom = 0;

  show('quick bench: loading');
  fetch('./board.json')
    .then(function (r) {
      return r.text();
    })
    .then(function (text) {
      boardText = text;
      // The preview opens the panel itself once it loads.
      return until(function () {
        return lastAt('quick_ready') != null;
      }, 30000);
    })
    .then(function () {
      var round = 0;
      var next = function () {
        if (round >= OPENS) return Promise.resolve();
        round++;
        show('quick bench: open ' + round);
        window.__emit('quick-hidden', null);
        return wait(300)
          .then(function () {
            // As quick.rs sends it: the board in the script, read as the script is.
            var t0 = performance.now();
            window.__quickOpen({ room: 600 }, JSON.parse(boardText));
            // The first open after load warms things up; it isn't counted.
            if (round > 1) opens.push(performance.now() - t0);
          })
          .then(next);
      };
      return next();
    })
    .then(function () {
      typedFrom = performance.now();
      var q = 0;
      var nextQuery = function () {
        if (q >= QUERIES.length) return Promise.resolve();
        var query = QUERIES[q++];
        var el = textarea();
        el.focus();
        type(el, '');
        var i = 0;
        var nextKey = function () {
          if (i >= query.length) return wait(900);
          i++;
          show('quick bench: typing "' + query.slice(0, i) + '"');
          var t0 = performance.now();
          type(el, query.slice(0, i));
          keysSync.push(performance.now() - t0);
          return painted()
            .then(function () {
              keysPaint.push(performance.now() - t0);
              return wait(KEY_GAP_MS);
            })
            .then(nextKey);
        };
        return nextKey().then(nextQuery);
      };
      return nextQuery();
    })
    .then(function () {
      fits = count('fit_quick', typedFrom);
      var typedLong = longTasks.filter(function (t) {
        return t[0] >= typedFrom;
      });
      var el = textarea();
      type(el, 'codealong');
      var round = 0;
      var next = function () {
        if (round >= PUSHES) return Promise.resolve(typedLong);
        round++;
        show('quick bench: push ' + round);
        return wait(120).then(function () {
          var t0 = performance.now();
          var b = JSON.parse(boardText);
          b.generatedAt = Date.now();
          var withSessions = b.lanes.filter(function (l) {
            return l.sessions.length > 0;
          });
          var lane = withSessions[round % withSessions.length];
          lane.lastActivity += round;
          lane.sessions[0].lastActivity += round;
          window.__emit('board', b);
          return drained()
            .then(painted)
            .then(function () {
              pushes.push(performance.now() - t0);
            })
            .then(next);
        });
      };
      return next();
    })
    .then(function (typedLong) {
      show(
        'done: open ' + stats(opens) + ' ms; key (sync) ' + stats(keysSync) + ' ms; key (painted) ' + stats(keysPaint) + ' ms over ' + keysSync.length + ' keys, ' + fits + ' fits, ' + typedLong.length + ' long tasks while typing (' + typedLong.map(function (t) {
          return t[1].toFixed(0);
        }).join(' ') + '); push (parsed to painted) ' + stats(pushes) + ' ms',
      );
    })
    .catch(function (e) {
      show('done: quick bench failed: ' + e.message);
    });
})();
