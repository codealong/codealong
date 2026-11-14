// Times the window's find (ui/WindowFind.tsx) over the main window as your board fills it: for a
// few words, how long a keystroke takes from typing to every match found, marked and the start
// brought into view (React's render, the walk over the window's text, the marks), which is as much
// as reading the window again as it changes costs, and more; drawing the boxes again as the page
// scrolls is the last part of it. A session is selected, so its panel and conversation are in the
// window too. It runs in Chrome: WebKit's cost to draw the one highlight is in
// .claude/rules/windows.md.
// Run on the real clock: PREVIEW_WAIT=90000 scripts/preview.sh sessions '' scripts/preview/window-find-bench.js
// The title ends up as "done: window find: N text nodes, M elements; <word>: median … ms, max … ms,
// K found, B boxed (<count>); …".
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var ROUNDS = 12;
  var WORDS = ['zqxj', 'session', 'the', 'e'];
  var show = function (text) {
    window.__benchTitle = text;
  };
  // The app rewrites the title as counts change, so ours is put back until Chrome reads it.
  window.setInterval(function () {
    if (window.__benchTitle) document.title = window.__benchTitle;
  }, 100);
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
        window.setTimeout(poll, 50);
      })();
    });
  };
  var field = function () { return document.querySelector('.ca-findbar input'); };
  var count = function () { var c = document.querySelector('.ca-findbar-count'); return c ? c.textContent : ''; };
  var setValue = function (text) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field(), text);
    field().dispatchEvent(new Event('input', { bubbles: true }));
  };
  var textNodes = function () {
    var n = 0;
    var w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    while (w.nextNode()) n++;
    return n;
  };
  var results = [];

  show('window find bench: loading');
  until(function () { return document.querySelectorAll('.ca-board-list [id^="row-"]').length > 20; }, 30000)
    .then(function () {
      var row = document.querySelector('.ca-board-list [id^="row-"]');
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return wait(1500);
    })
    .then(function () {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', code: 'KeyF', metaKey: true, bubbles: true, cancelable: true }));
      return until(function () { return !!field(); }, 3000);
    })
    .then(function () {
      var chain = Promise.resolve();
      WORDS.forEach(function (word) {
        var times = [];
        for (var i = 0; i < ROUNDS; i++) {
          chain = chain
            .then(function () {
              setValue('');
              return wait(120);
            })
            .then(function () {
              var t0 = performance.now();
              setValue(word);
              times.push(performance.now() - t0);
              return wait(120);
            });
        }
        chain = chain.then(function () {
          times.sort(function (a, b) { return a - b; });
          var layer = document.querySelector('.ca-findmarks');
          results.push(word + ': median ' + times[Math.floor(times.length / 2)].toFixed(1) + ' ms, max ' + times[times.length - 1].toFixed(1) + ' ms, ' + (layer ? layer.dataset.shown + ' found, ' + layer.children.length + ' boxed' : 'none') + ' (' + count() + ')');
          show('window find bench: ' + results.length + ' of ' + WORDS.length);
        });
      });
      return chain;
    })
    .then(function () {
      show('done: window find: ' + textNodes() + ' text nodes, ' + document.querySelectorAll('*').length + ' elements; ' + results.join('; '));
    })
    .catch(function (e) {
      show('done: window find bench failed: ' + e.message);
    });
})();
