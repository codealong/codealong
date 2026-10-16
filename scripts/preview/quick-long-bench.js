// Times typing into the quick panel at the end of a long prompt: for each length, the prompt is
// put in the input at once, then keys are typed after it one by one, each timed from its input
// event to the page drawing it. Run on the real clock:
//   PREVIEW_WAIT=180000 scripts/preview.sh quick '' scripts/preview/quick-long-bench.js
// The title ends up as "done: <length> chars: key (sync) …, key (painted) …" for each length,
// each a median, p90 and max in ms. A key should cost the same whatever came before it.
(function () {
  var LENGTHS = [0, 1000, 4000, 12000];
  var KEYS = 30;
  var KEY_GAP_MS = 60;
  var PARAGRAPH =
    'The quick panel should keep up with typing however long the prompt gets. When I paste a long description of a bug, ' +
    'with the steps to reproduce it, what I expected and what happened instead, every key after it lags behind my fingers. ' +
    'It would be good to know where the time goes: the search over sessions, todos and PRs, the input growing, the list, or the window. ';
  var show = function (text) {
    window.__benchTitle = text;
  };
  window.setInterval(function () {
    if (window.__benchTitle) document.title = window.__benchTitle;
  }, 100);

  var sent = [];
  var inner = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    sent.push([cmd, performance.now()]);
    // What sessions said answers at once with nothing, as the engine's index would for a prompt.
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    return inner.call(this, cmd, args, options);
  };
  var lastAt = function (cmd) {
    for (var i = sent.length - 1; i >= 0; i--) if (sent[i][0] === cmd) return sent[i][1];
    return null;
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
        window.setTimeout(poll, 2);
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
  var textarea = function () {
    return document.querySelector('.ca-quick-input textarea');
  };
  var setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
  var type = function (el, value) {
    setValue.call(el, value);
    el.setSelectionRange(value.length, value.length);
    el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value.slice(-1) }));
  };
  var prompt = function (n) {
    var s = '';
    while (s.length < n) s += PARAGRAPH;
    return s.slice(0, n);
  };

  var results = [];
  show('quick long bench: loading');
  until(function () {
    return lastAt('quick_ready') != null;
  }, 30000)
    .then(function () {
      var l = 0;
      var nextLength = function () {
        if (l >= LENGTHS.length) return Promise.resolve();
        var n = LENGTHS[l++];
        var el = textarea();
        el.focus();
        var base = prompt(n);
        type(el, base);
        var sync = [];
        var paint = [];
        return wait(1500).then(function () {
          var typed = '';
          var i = 0;
          var nextKey = function () {
            if (i >= KEYS) {
              results.push(n + ' chars: key (sync) ' + stats(sync) + ', key (painted) ' + stats(paint));
              type(el, '');
              return wait(800).then(nextLength);
            }
            typed += 'abcdefghij klmnopq rstuvwxyz '[i % 29];
            i++;
            show('quick long bench: ' + n + ' chars, key ' + i);
            var t0 = performance.now();
            type(el, base + typed);
            sync.push(performance.now() - t0);
            return painted()
              .then(function () {
                paint.push(performance.now() - t0);
                return wait(KEY_GAP_MS);
              })
              .then(nextKey);
          };
          return nextKey();
        });
      };
      return nextLength();
    })
    .then(function () {
      show('done: ' + results.join('; '));
    })
    .catch(function (e) {
      show('done: quick long bench failed: ' + e.message);
    });
})();
