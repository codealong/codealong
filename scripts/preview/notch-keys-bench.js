// Times typing into the box of a session shown in the notch: the notch is opened on
// PREVIEW_TRANSCRIPT's session (its real conversation), which must have a pill on the bar (the
// bench names the ones there when it hasn't), then keys are typed into its box one by one, each
// timed from its input event to the script's return, to style and layout (forced, to time them
// apart) and to the page drawing it. Run on the real clock:
//   PREVIEW_TRANSCRIPT=<key> PREVIEW_SIZE=1512,900 PREVIEW_WAIT=120000 \
//     scripts/preview.sh notch '' scripts/preview/notch-keys-bench.js
// The title ends up as "done: key (sync) …; key (laid out) …; key (painted) …", each a median,
// p90 and max in ms, with how many long tasks typing ran into and the commands it sent. On
// 2026-11-16 a key cost 0.5 ms of script and 8 ms to its frame in Chrome, and 1 ms and 2 ms in an
// offscreen WKWebView (which waits on no display), on a card of ~390 elements.
(function () {
  var TEXT = 'Can you give me the name of the org that did this and when it started?';
  var ROUNDS = 3;
  var KEY_GAP_MS = 90;
  var show = function (text) {
    window.__benchTitle = text;
  };
  // The app rewrites the title as counts change, so ours is put back until Chrome reads it.
  window.setInterval(function () {
    if (window.__benchTitle) document.title = window.__benchTitle;
  }, 100);

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
        window.setTimeout(poll, 20);
      })();
    });
  };
  var painted = function () {
    return new Promise(function (r) {
      requestAnimationFrame(function () {
        window.setTimeout(r, 0);
      });
    });
  };
  var stats = function (xs) {
    var s = xs.slice().sort(function (a, b) {
      return a - b;
    });
    var at = function (q) {
      return s[Math.min(s.length - 1, Math.floor(q * s.length))];
    };
    return 'median ' + at(0.5).toFixed(1) + ', p90 ' + at(0.9).toFixed(1) + ', max ' + s[s.length - 1].toFixed(1);
  };
  var setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
  var type = function (el, value) {
    setValue.call(el, value);
    el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value.slice(-1) }));
  };
  var box = function () {
    return document.querySelector('.ca-notch-session .ca-composer textarea');
  };

  var wanted = null;
  show('notch bench: loading');
  fetch('./transcript-key.json')
    .then(function (r) {
      return r.ok ? r.json() : null;
    })
    .then(function (k) {
      wanted = k && k.key;
      return until(function () {
        return document.querySelector('.ca-notch-shape') && document.querySelector('.ca-notch-count');
      }, 30000);
    })
    .then(function () {
      // The notch opens as the pointer comes over it.
      document.querySelector('.ca-notch-shape').dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      return until(function () {
        return document.querySelectorAll('.ca-notch-body .ca-hud-pill[data-key]').length > 0;
      }, 10000);
    })
    .then(function () {
      // A click while it opens is taken for one that puts it away.
      return wait(500);
    })
    .then(function () {
      var pills = Array.prototype.slice.call(document.querySelectorAll('.ca-notch-body .ca-hud-pill[data-key]'));
      var pill = pills.filter(function (p) {
        return p.dataset.key === wanted;
      })[0];
      if (!pill) throw new Error('no pill for ' + wanted + ' on the bar: name one of ' + pills.map(function (p) { return p.dataset.key; }).join(', '));
      pill.click();
      return until(box, 10000);
    })
    .then(function () {
      // Its conversation in.
      return wait(2500);
    })
    .then(function () {
      var from = performance.now();
      var callsFrom = window.__calls.length;
      var sync = [];
      var layout = [];
      var paint = [];
      var round = 0;
      var nextRound = function () {
        if (round >= ROUNDS) return Promise.resolve();
        round++;
        var el = box();
        if (!el) throw new Error('the box went before round ' + round + ' (' + sync.length + ' keys typed): ' + ((document.querySelector('.ca-notch') || {}).className || 'no notch'));
        el.focus();
        type(el, '');
        var i = 0;
        var nextKey = function () {
          if (i >= TEXT.length) return wait(500);
          i++;
          show('notch bench: round ' + round + ', key ' + i);
          var el = box();
          var t0 = performance.now();
          type(el, TEXT.slice(0, i));
          sync.push(performance.now() - t0);
          // Style and layout, which the frame would do: forced here to be timed apart.
          void document.body.offsetHeight;
          layout.push(performance.now() - t0);
          return painted()
            .then(function () {
              paint.push(performance.now() - t0);
              return wait(KEY_GAP_MS);
            })
            .then(nextKey);
        };
        return nextKey().then(nextRound);
      };
      return nextRound().then(function () {
        var typedLong = longTasks.filter(function (t) {
          return t[0] >= from;
        });
        // The commands typing sent, by name.
        var sent = {};
        window.__calls.slice(callsFrom).forEach(function (c) {
          var name = c[0] === 'plugin:event|emit' || c[0] === 'plugin:event|emit_to' ? c[0] + ' ' + (c[1] && c[1].event) : c[0];
          sent[name] = (sent[name] || 0) + 1;
        });
        show(
          'done: key (sync) ' + stats(sync) + ' ms; key (laid out) ' + stats(layout) + ' ms; key (painted) ' + stats(paint) + ' ms over ' + sync.length + ' keys, ' + typedLong.length + ' long tasks while typing (' +
            typedLong
              .map(function (t) {
                return t[1].toFixed(0);
              })
              .join(' ') +
            '); ' + document.querySelectorAll('.ca-notch-session *').length + ' elements in the card; sent ' + JSON.stringify(sent),
        );
      });
    })
    .catch(function (e) {
      show('done: notch bench failed: ' + e.message);
    });
})();
