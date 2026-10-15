// Checks a session's side conversations and helpers in Sessions and its header, on the board
// sides-init.js makes (boardLists.ts `sideCounts`, lanes.ts `sidesListed`, `foldsOf`,
// SessionsView.tsx `family`, LaneRow.tsx `SidesChip`, `SideRow`, `FoldRow`, lineage.ts
// `cameLines`): the one answering is listed under the session, set in as a helper at work is,
// with the working glyph; the session's row counts all three on a chip of their own, whose card
// names each and where it stands, and a click on one there selects the session and shows that one
// above its box, to read; a click on the row of the one answering selects the session and shows
// it there; a double-click on the chip lists them all under it, and a row at the end says how
// many would leave the list and takes them away, as a double-click on the helpers' chip and its
// row do for the helper that's done; once the answer comes, its row leaves the list; and the
// session's header names what came of it, the latest first: the side conversation, then the
// rest under "+N more", the helpers among them.
//   PREVIEW_INIT=scripts/preview/sides-init.js PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/sides-check.js
(function () {
  var dblclick = function (el) { el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true })); };
  var press = function (el) { el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 })); };
  (async function () {
    try {
      var m = await until(function () { return window.__sidesMade && all('.ca-sessionlist .ca-lane.is-agent').length > 0 && window.__sidesMade; }, 30000, 'the board with side conversations');
      var bossRow = function () { return document.getElementById('row-' + m.lane + '::' + m.boss.key); };
      var helperRow = function (i) { return document.getElementById('row-' + m.lane + '::' + m.helpers[i].key); };
      // What's listed under the session: the rows after its own, while they're set in.
      var under = function () {
        var out = [];
        for (var el = bossRow() && bossRow().nextElementSibling; el && el.classList.contains('is-nested'); el = el.nextElementSibling) out.push(el);
        return out;
      };
      var sideRows = function () { return under().filter(function (el) { return el.classList.contains('is-side'); }); };
      var folds = function () { return under().filter(function (el) { return el.classList.contains('is-fold'); }); };
      await until(function () { return bossRow() && sideRows().length > 0; }, 10000, 'the side conversation answering under its session');

      var side = sideRows()[0];
      check('the one answering is listed under its session, set in, and only it', sideRows().length === 1 && /Can we parallelize this\?/.test(side.textContent) && /answering/.test(side.textContent), sideRows().map(function (el) { return el.textContent; }).join(' | '));
      check('with the working glyph', !!q('.ca-glyph--working', side));
      check('the helper at work is listed under it too, the one done isn’t', !!helperRow(0) && helperRow(0).classList.contains('is-nested') && !helperRow(1));

      var chip = function () { return q('.ca-sideschip', bossRow()); };
      check('the session’s row counts its side conversations on a chip of their own', !!chip() && chip().textContent.trim() === '3' && !!q('.ca-glyph--working', chip()), chip() && chip().textContent);

      // Its card: each, the one answering first.
      chip().closest('.ca-helperschip-anchor').dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      var card = await until(function () { return q('.ca-helperscard'); }, 3000, 'the side conversations card');
      var text = function (el, sel) { var x = el && q(sel, el); return x ? x.textContent : ''; };
      var cardRows = all('.ca-helperscard-row', card);
      check('its card names them as side conversations', /^Its 3 side conversations/.test(text(card, '.ca-hovercard-head')) && /throwaway copy/.test(text(card, '.ca-hovercard-meta')), text(card, '.ca-hovercard-head'));
      check('a row for each, the one answering first, saying where it stands', cardRows.length === 3 && /^Answering for /.test(text(cardRows[0], '.ca-helperscard-stands')) && /^Answered /.test(text(cardRows[1], '.ca-helperscard-stands')) && text(cardRows[1], '.ca-helperscard-name') === 'Why a queue?', cardRows.map(function (x) { return text(x, '.ca-helperscard-name') + ' / ' + text(x, '.ca-helperscard-stands'); }).join(' | '));
      check('with no arrow to open one elsewhere', cardRows.every(function (x) { return !q('.ca-helperscard-open', x); }));
      q('.ca-helperscard-main', cardRows[1]).dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      var strip = function () { return q('.ca-sidestrip'); };
      await until(function () { return bossRow().classList.contains('is-selected') && strip() && /Why a queue\?/.test(strip().textContent); }, 5000, 'the kept one above the session’s box');
      check('a click on one there selects the session and shows that one above its box, to read', /closed/.test(strip().textContent) && /And without one\?/.test(strip().textContent), strip().textContent.slice(0, 160));

      // Its row: the session selected, and the one answering above its box.
      press(sideRows()[0]);
      await until(function () { return strip() && /Can we parallelize this\?/.test(strip().textContent); }, 5000, 'the one answering above the box');
      check('a click on its row shows the one answering above the session’s box', bossRow().classList.contains('is-selected') && !/closed/.test(strip().textContent), strip().textContent.slice(0, 160));
      check('and the box asks it', !!q('.ca-composer.is-aside'));

      // The chip's double-click lists them all, and the row at the end takes them away.
      dblclick(chip());
      await until(function () { return sideRows().length === 3; }, 5000, 'every side conversation listed');
      check('a double-click lists them all under it, the latest started first', sideRows().map(function (el) { return q('.ca-agentline-title', el).textContent; }).join(' | ') === 'Can we parallelize this? | Why a queue? | Is the queue tested?' && chip().classList.contains('is-shown'), sideRows().map(function (el) { return el.textContent; }).join(' | '));
      check('and a row at the end says how many would leave the list', folds().length === 1 && folds()[0] === under()[under().length - 1] && folds()[0].textContent === 'Hide 2 answered side conversations', folds().map(function (el) { return el.textContent; }).join(' | '));
      q('button', folds()[0]).click();
      await until(function () { return sideRows().length === 1; }, 5000, 'only the one answering again');
      check('which takes them away again', folds().length === 0 && !chip().classList.contains('is-shown'));

      dblclick(q('.ca-helperschip', bossRow()));
      await until(function () { return !!helperRow(1); }, 5000, 'the helper that’s done listed');
      check('the helpers’ chip lists them all too, with a row at the end for the one done', folds().length === 1 && folds()[0].textContent === 'Hide 1 finished helper', folds().map(function (el) { return el.textContent; }).join(' | '));
      q('button', folds()[0]).click();
      await until(function () { return !helperRow(1); }, 5000, 'the helper that’s done gone again');
      check('which takes it away again', !!helperRow(0) && folds().length === 0);

      // The session's header: what came of it, the latest first.
      var kin = function () { return all('.ca-sessinsp-head .ca-sessinsp-kid'); };
      var more = function () { return q('.ca-sessinsp-head .ca-sessinsp-kids-more'); };
      check('the header names the latest of what came of it, the side conversation', kin().length === 1 && !!q('.ca-sessinsp-kin.is-side', kin()[0]) && /Can we parallelize this\?/.test(kin()[0].textContent), kin().map(function (el) { return el.textContent; }).join(' | '));
      check('and has the rest under +4 more, the helpers among them', !!more() && more().textContent === '+4 more' && /Its helper Review the queue change/.test(more().title) && /On the side: Is the queue tested\?/.test(more().title), more() && more().title);
      more().click();
      await until(function () { return kin().length === 5; }, 5000, 'every one in the header');
      var lines = kin().map(function (el) { return el.textContent; });
      check('which opens them all, the latest started first', /Review the queue change/.test(lines[1]) && /Check the migration/.test(lines[2]) && /Why a queue\?/.test(lines[3]) && /Is the queue tested\?/.test(lines[4]), lines.join(' | '));
      more().click();

      // The answer comes: it's answered, and leaves the list.
      var open = window.__sides[m.boss.key];
      var answered = Object.assign({}, open, { running: false, turns: [Object.assign({}, open.turns[0], { answer: 'Yes, by repo.', done: true })] });
      window.__sides[m.boss.key] = answered;
      window.__emit('side', answered);
      await window.__pushSides();
      await until(function () { return sideRows().length === 0; }, 5000, 'its row gone once answered');
      check('once answered, its row leaves the list, and the chip has no working glyph', !q('.ca-glyph--working', chip()) && chip().textContent.trim() === '3', chip().textContent);
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
