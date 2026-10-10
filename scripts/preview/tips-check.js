// Checks the tips the windows show while they wait on what they read (Tip.tsx, tips.ts): one
// comes under what the window says it's reading, its keys on caps and the quick panel's shortcut
// as config.toml sets it; Next tip goes through the place's tips, each once before any comes
// round again, and each is kept as shown, for the next wait in any window. (That a shortcut
// that's off leaves its tips out is tips.test.ts's.)
//   PREVIEW_LOADING=1 PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/tips-check.js
//   PREVIEW_LOADING=1 PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/tips-check.js
//   PREVIEW_LOADING=1 PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/tips-check.js
(function () {
  var quick = !!window.__CODEALONG_QUICK__;
  var where = quick ? 'quick panel' : window.__CODEALONG_WAITING__ ? 'waiting window' : 'main window';
  // Under the line that says what's being read, in each window.
  var under = quick ? '.ca-quick-scroll' : window.__CODEALONG_WAITING__ ? '.ca-waiting-empty' : '.ca-board-loading';
  var tip = function () { return document.querySelector(under + ' .ca-tip'); };
  var id = function () { var t = tip(); return t ? t.getAttribute('data-tip') : ''; };
  var said = function () { var t = tip(); return t ? t.querySelector('.ca-tip-text').textContent : ''; };
  var shown = function () { try { return JSON.parse(localStorage.getItem('codealong.tips.shown') || '[]'); } catch (e) { return []; } };
  var words = /\b(Cmd|Ctrl|Alt|Option|Shift|Enter|Backspace)\b/;

  say('tips check: the ' + where);
  until(function () { return document.body.textContent.indexOf('Reading sessions…') >= 0; }, 20000)
    .then(function () { return until(tip, 8000); })
    .then(function () {
      var first = said();
      check('a tip comes under the reading line', first.length > 20, first);
      check('it says it is a tip', tip().querySelector('.ca-label').textContent === 'Tip');
      check('and is kept as shown', shown()[shown().length - 1] === id(), JSON.stringify(shown()) + ' / ' + id());
      // Through every tip of this place, with Next tip: each once, then round again.
      var ids = [id()];
      var texts = [first];
      var step = function () {
        var before = id();
        tip().querySelector('.ca-tip-next').click();
        return until(function () { return id() !== before; }, 2000).then(function () {
          if (ids.indexOf(id()) >= 0) return null;
          ids.push(id());
          texts.push(said());
          return ids.length < 80 ? step() : null;
        });
      };
      return step().then(function () {
        note(ids.length + ' tips here: ' + ids.join(', '));
        check('Next tip goes through several tips', ids.length >= 5, String(ids.length));
        check('and comes round to the first once all have shown', id() === ids[0], id());
        check('each kept as shown', ids.every(function (t) { return shown().indexOf(t) >= 0; }), JSON.stringify(shown()));
        check('keys are on caps, never written as words', texts.every(function (s) { return !words.test(s); }), texts.filter(function (s) { return words.test(s); }).join(' | '));
        check('every tip ends a sentence', texts.every(function (s) { return /\.$/.test(s); }), texts.filter(function (s) { return !/\.$/.test(s); }).join(' | '));
        if (!quick) {
          var named = texts.filter(function (s) { return s.indexOf('⌥⌘/') === 0; });
          check('the quick panel’s shortcut is said as config.toml sets it', named.length === 1, texts.join(' | '));
        }
      });
    })
    .then(done, stopped);
})();
