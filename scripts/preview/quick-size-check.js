// Checks that the quick panel keeps one height whatever it lists (useQuickSize.ts): with nothing
// typed, with a search that finds a few and one that finds nothing, and with a long prompt, which
// takes its room from the list; that the input grows to its tallest and goes back to one line when
// emptied and on the next open; that the open says the same height every time; and that the bar's
// menus open within the card, over the list.
//
//   scripts/preview.sh quick '' scripts/preview/quick-size-check.js
(function () {
  var card = function () { return q('.ca-quick'); };
  var input = function () { return q('.ca-quick-input textarea'); };
  var height = function (el) { return el.getBoundingClientRect().height; };
  var LONG = new Array(60).join('A long prompt, pasted with the steps that show the bug and what should happen instead.\n');

  say('quick size check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel')
    .then(function () { return wait(200); })
    .then(function () {
      var tall = height(card());
      var oneLine = height(input());
      var said = calls('quick_ready').map(function (c) { return c[1].height; });
      check('the open says the card and its margins', said[said.length - 1] === tall + 40, JSON.stringify({ said: said, card: tall }));
      var sizes = [['nothing typed', tall]];
      var step = function (words) {
        return function () {
          type(input(), words);
          return wait(400).then(function () { sizes.push([JSON.stringify(words.slice(0, 20)), height(card())]); });
        };
      };
      return step('codealong')()
        .then(step('zzqx vbnmw'))
        .then(step(LONG))
        .then(function () {
          check('the card keeps one height whatever it lists', sizes.every(function (s) { return s[1] === tall; }), JSON.stringify(sizes));
          var grown = height(input());
          // Eight lines and its padding, against one line and its padding.
          check('a long prompt grows the input to its tallest', grown > oneLine * 4, JSON.stringify({ oneLine: oneLine, grown: grown }));
          check('and takes its room from the list', q('.ca-quick-list').getBoundingClientRect().top >= q('.ca-quick-head').getBoundingClientRect().bottom, '');
          type(input(), LONG + 'one more');
          return wait(100).then(function () {
            check('a key at its tallest leaves it there', height(input()) === grown, JSON.stringify({ grown: grown, now: height(input()) }));
            type(input(), '');
            return wait(100);
          }).then(function () {
            check('emptied, it goes back to one line', height(input()) === oneLine, JSON.stringify({ oneLine: oneLine, now: height(input()) }));
            type(input(), LONG);
            return wait(100);
          }).then(function () {
            window.__quickOpen({}, null);
            return wait(200);
          }).then(function () {
            check('the next open starts at one line', height(input()) === oneLine && height(card()) === tall, JSON.stringify({ oneLine: oneLine, now: height(input()), card: height(card()) }));
            q('.ca-quick-agent .ca-quick-chip').click();
            return until(function () { return !!q('.ca-quick-bar .ca-picker'); }, 3000, 'the agent menu');
          }).then(function () {
            var menu = q('.ca-quick-bar .ca-picker').getBoundingClientRect();
            var box = card().getBoundingClientRect();
            var bar = q('.ca-quick-bar').getBoundingClientRect();
            check('the agent menu opens over the list, within the card', menu.top >= box.top && menu.bottom <= bar.top, JSON.stringify({ menu: [menu.top, menu.bottom], card: [box.top, box.bottom], bar: bar.top }));
          });
        });
    })
    .then(done, stopped);
})();
