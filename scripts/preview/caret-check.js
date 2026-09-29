// Checks the watch for a lost text cursor (caret.ts): a key typed into a text field while the page
// says it isn't focused asks Codealong to hand the page its focus again, then logs whether that
// worked; once in a while at most, and never for a focused page or a key outside a text field.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/caret-check.js
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var focused = true;
  document.hasFocus = function () { return focused; };
  var key = function (el) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true })); };

  say('caret check: waiting for a text field');
  until(function () { return document.querySelector('textarea, input[type="text"], input:not([type])'); }, 20000)
    .then(function () {
      var field = document.querySelector('textarea, input[type="text"], input:not([type])');
      key(field);
      check('a focused page asks nothing', calls('page_unfocused').length === 0);
      focused = false;
      var button = document.querySelector('button');
      if (button) key(button);
      check('a key outside a text field asks nothing', calls('page_unfocused').length === 0);
      key(field);
      return wait(300).then(function () {
        check('an unfocused page with typing asks for its focus', calls('page_unfocused').length === 1);
        var marks = calls('perf_mark').map(function (c) { return c[1].line; }).filter(function (l) { return l.indexOf('WebKit had as unfocused') >= 0; });
        check('logs how the repair went', marks.length === 1 && marks[0].indexOf('still unfocused') > 0, marks.join(' | '));
        key(field);
        key(field);
        return wait(100);
      });
    })
    .then(function () { check('not on every key', calls('page_unfocused').length === 1); })
    .then(function () { say('done: ' + out.join(' · ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(' · ')); });
})();
