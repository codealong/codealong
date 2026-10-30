// Checks that the quick panel holding a prompt folds rather than hides as another window takes
// the keyboard (quick.rs `put_aside`, Quick.tsx `__quickFold`, `QuickFold`): a word or two of a
// search doesn't fold it; a prompt folds it to a chip that says its first line and is measured
// for its window; a click on the chip opens the panel again as it was, the caret where it was; a
// click in the shadow round the card leaves a panel holding a prompt be; folded, ⌘Q puts it away
// rather than reaching Codealong's menu; and × on the chip puts it away.
//
//   scripts/preview.sh quick '' scripts/preview/quick-fold-check.js
(function () {
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return q('.ca-quick-fold'); };
  var card = function () { return q('.ca-quick'); };
  var PROMPT = 'Fix the pangolin pen so the gate shuts\nand the latch holds';

  say('quick fold check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel')
    .then(function () { return wait(200); })
    .then(function () {
      check('with nothing typed there is nothing to hold', window.__quickFold() === null, 'folded');
      type(input(), 'pangolin pen');
      return wait(100);
    })
    .then(function () {
      check('a search of a word or two is nothing to hold', window.__quickFold() === null, 'folded');
      check('so no chip shows', !chip(), 'a chip');
      type(input(), PROMPT);
      return wait(100);
    })
    .then(function () {
      input().setSelectionRange(8, 8);
      var size = window.__quickFold();
      check('a prompt folds the panel, and says its window’s size', !!size && size.length === 2 && size[0] > 100 && size[1] > 40, JSON.stringify(size));
      check('the card goes', !!card() && getComputedStyle(card()).display === 'none', card() && getComputedStyle(card()).display);
      var said = chip() ? chip().textContent : '';
      check('the chip says the prompt’s first line', said.indexOf('Fix the pangolin pen so the gate shuts') >= 0 && said.indexOf('latch') < 0, said);
      var r = chip() && chip().getBoundingClientRect();
      check('the size is the chip’s with room for its shadow', !!r && !!size && size[0] === Math.ceil(r.width) + 40 && size[1] === Math.ceil(r.height) + 40, JSON.stringify([r && r.width, r && r.height, size]));
      var before = calls('show_quick').length;
      q('.ca-quick-fold-open').click();
      return wait(100).then(function () { return before; });
    })
    .then(function (before) {
      var asked = calls('show_quick').slice(before);
      check('a click on the chip opens the panel again as it was', asked.length === 1 && !!asked[0][1] && asked[0][1].context.keep === true, JSON.stringify(asked));
      // Codealong answers that with an open that keeps what's there.
      window.__quickOpen({ keep: true }, null);
      return wait(100);
    })
    .then(function () {
      check('the card is back', !chip() && getComputedStyle(card()).display !== 'none', 'folded still');
      check('with the prompt as written', input().value === PROMPT, input().value);
      check('the caret where it was', input().selectionStart === 8, String(input().selectionStart));
      q('.ca-quick-root').dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      return wait(100);
    })
    .then(function () {
      check('a click in the shadow leaves a panel holding a prompt be', calls('hide_quick').length === 0 && !chip(), String(calls('hide_quick').length));
      window.__quickFold();
      return wait(100);
    })
    .then(function () {
      var quit = new KeyboardEvent('keydown', { key: 'q', code: 'KeyQ', metaKey: true, bubbles: true, cancelable: true });
      window.dispatchEvent(quit);
      check('folded, ⌘Q is the panel’s, not Codealong’s menu', quit.defaultPrevented && calls('hide_quick').length === 1, String(calls('hide_quick').length));
      return wait(100);
    })
    .then(function () {
      q('.ca-quick-fold .ca-btn').click();
      return wait(100);
    })
    .then(function () {
      check('× on the chip puts the panel away', calls('hide_quick').length === 2, String(calls('hide_quick').length));
      done();
    })
    .catch(stopped);
})();
