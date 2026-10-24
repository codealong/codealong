// Checks Where reviews run in Settings › Automation (config.rs `ReviewStart`, review.ts
// `reviewsRunIn`), with reviews on and Cursor named as the reviewer of your own pull requests
// (settings-review-start-init.js). A file without it reads as never in a terminal, so Cursor,
// whose Background is a terminal of its own, is said not to be able to start one. Picking where
// saves `[reviews] start`, and the line under the reviewer follows: Background puts Cursor in the
// background (tmux with no window, where this Mac has tmux; a terminal behind your windows where
// it doesn't), your terminal in the terminal app.
//   PREVIEW_INIT=scripts/preview/settings-review-start-init.js PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/review-start-check.js
(function () {
  var field = function (words) { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && words.test(s.textContent); })[0]; };
  var where = function () { var f = field(/^Where reviews run/); return f && q('select', f); };
  var says = function (words) { var f = field(words); var s = f && q(':scope > small', f); return s ? s.textContent : ''; };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var lastSave = function () { var c = calls('save_config'); return c.length ? c[c.length - 1][1].config : null; };

  (async function () {
    try {
      await until(where, 20000, 'Settings › Automation has no Where reviews run');
      var select = where();
      check('a file without it reviews never in a terminal', select.value === 'no-terminal', select.value);
      var options = all('option', select).map(function (o) { return o.value; });
      check('it offers no terminal, Codealong, Background and your terminal', options.join() === 'no-terminal,codealong,background,terminal', options.join());
      check('and says who can’t review so', /Antigravity and Cursor can do neither/.test(says(/^Where reviews run/)), says(/^Where reviews run/));

      var own = function () { return says(/^Reviewer of your pull requests/); };
      // The accounts come after the field; named but missing from this Mac's, the line says what
      // Auto does instead.
      var cursor = await until(function () { return /cursor/.test(own()); }, 5000, '').then(
        function () { return true; },
        function () { return false; },
      );
      if (!cursor) {
        note('no Cursor account among the start options to name as a reviewer: ' + own());
      } else {
        check('Cursor, named, can’t start one without a terminal', /cursor can’t start a review without a terminal: pick another reviewer, or change where reviews run\./.test(own()), own());
      }

      pick(where(), 'background');
      await until(function () { var c = lastSave(); return c && c.reviews.start === 'background'; }, 5000, 'picking Background saved no [reviews] start');
      check('picking where saves [reviews] start', lastSave().reviews.start === 'background', JSON.stringify(lastSave().reviews));
      if (cursor) {
        await until(function () { return /It runs (in a terminal behind your windows|in the background)\./.test(own()); }, 5000, 'with Background, the line under Cursor says: ' + own());
        check('with Background, Cursor runs in the background or a terminal behind your windows', true, own());
      }
      await until(function () { return where() && where().value === 'background'; }, 5000, 'the select went back after the save');

      pick(where(), 'terminal');
      await until(function () { var c = lastSave(); return c && c.reviews.start === 'terminal'; }, 5000, 'picking your terminal saved no [reviews] start');
      if (cursor) {
        await until(function () { return /It runs in (?!a terminal behind)[^.]+\./.test(own()); }, 5000, 'in your terminal, the line under Cursor says: ' + own());
        check('in your terminal, Cursor runs in the terminal app', !/behind/.test(own()), own());
      }
      check('your terminal says it comes forward, as new terminals open', /New terminals open as says \(General\), and comes forward/.test(says(/^Where reviews run/)), says(/^Where reviews run/));
      return done();
    } catch (e) {
      return stopped(e);
    }
  })();
})();
