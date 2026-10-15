// Checks helpers in Sessions (SessionsView.tsx `sessionSections`, boardLists.ts `placeHelpers`),
// on the board helpers-init.js makes: the helpers that work or wait on you are listed under the
// session that started them, set in, each on one line with no divider above it and a thread from
// the session's status into its own (LaneRow.css), and the finished one isn't; the session's chip
// has each helper's status glyph, the one waiting on you first, in the attention color (LaneRow.tsx
// `HelpersChip`); a double-click on it lists the finished one too and remembers that, and another
// takes it away; resting on it brings up its card (HelpersCard.tsx), a row for each helper saying
// where it stands and who runs it, whose right-click is that helper's menu and whose click lists
// the finished one under the session and shows it; the helper that waits on you takes the session
// into Needs you with it. The session's panel lists its helpers under its
// title, and a helper's panel names the session as the one it helps.
//   PREVIEW_INIT=scripts/preview/helpers-init.js PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/helpers-check.js
(function () {
  var rowOf = function (title) {
    return all('.ca-sessionlist .ca-lane.is-agent').filter(function (r) { return (q('.ca-agentline-title, .ca-agentline-text', r) || r).textContent.indexOf(title) >= 0; })[0];
  };
  var select = async function (row) {
    row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    await wait(600);
  };
  (async function () {
    try {
      var h = await until(function () { return window.__helpers && all('.ca-sessionlist .ca-lane.is-agent').length > 0 && window.__helpers; }, 30000, 'the board with helpers');
      var boss = await until(function () { return rowOf(h.boss.title); }, 10000, 'the session that started them');
      var listed = all('.ca-sessionlist .ca-lane.is-agent');
      var at = listed.indexOf(boss);
      var kidRows = h.kids.map(function (k) { return rowOf(k.title); });
      check('the working helper and the one waiting on you are listed', !!kidRows[0] && !!kidRows[1]);
      check('right under the session that started them', listed[at + 1] && listed[at + 2] && [listed[at + 1], listed[at + 2]].every(function (r) { return r === kidRows[0] || r === kidRows[1]; }), listed.slice(at, at + 3).map(function (r) { return r.className; }).join(' / '));
      check('set in', kidRows.slice(0, 2).every(function (r) { return r && r.classList.contains('is-nested'); }) && !boss.classList.contains('is-nested'));
      var bossGlyph = q('.ca-seg-glyph .ca-glyph', boss).getBoundingClientRect();
      var kidGlyph = kidRows[1] && q('.ca-seg-glyph .ca-glyph', kidRows[1]).getBoundingClientRect();
      check('their status sits in from the session’s', kidGlyph && kidGlyph.left > bossGlyph.left + 8, kidGlyph && Math.round(kidGlyph.left - bossGlyph.left) + 'px');
      var shown = kidRows.slice(0, 2);
      check('each on one line, with no PR or diff', shown.every(function (r) { return r && !q('.ca-lane-line2', r) && !q('.ca-diff', r); }));
      check('naming its worktree only when it isn’t the session’s', shown.every(function (r) {
        var kid = h.kids.filter(function (k) { return r && rowOf(k.title) === r; })[0];
        return !!kid && !!q('.ca-wtchip', r) === (kid.lane !== h.boss.lane);
      }), h.kids.map(function (k) { return k.lane === h.boss.lane ? 'same' : 'other'; }).join(' '));
      // One block: no divider between the session and its helpers, only after the last.
      var clear = function (r) { return getComputedStyle(r).borderBottomColor === 'rgba(0, 0, 0, 0)'; };
      check('no divider between the session and its helpers, only after the last', clear(listed[at]) && clear(listed[at + 1]) && !clear(listed[at + 2]));
      // The thread: down from under the session's glyph, and into each helper's at its middle.
      var line = function (r, which) {
        var seg = q('.ca-seg-glyph', r);
        var st = getComputedStyle(seg, which);
        return st.content === 'none' ? null : { x: seg.getBoundingClientRect().left + parseFloat(st.left) + parseFloat(st.borderLeftWidth) / 2, style: st };
      };
      var down = line(boss, '::after');
      var bossMid = bossGlyph.left + bossGlyph.width / 2;
      check('a thread runs down from under the session’s status', !!down && Math.abs(down.x - bossMid) < 1, down && (down.x - bossMid).toFixed(2) + 'px off');
      check('and turns into each helper’s', shown.every(function (r) {
        var hook = line(r, '::before');
        var g = q('.ca-seg-glyph .ca-glyph', r).getBoundingClientRect();
        var seg = q('.ca-seg-glyph', r).getBoundingClientRect();
        var end = hook && seg.top + parseFloat(hook.style.top) + parseFloat(hook.style.height) + parseFloat(hook.style.borderBottomWidth) / 2;
        return !!hook && Math.abs(hook.x - bossMid) < 1 && Math.abs(end - (g.top + g.height / 2)) < 1;
      }));
      check('past the first helper to the last', !!line(listed[at + 1], '::after') && !line(listed[at + 2], '::after'));
      check('the finished one isn’t listed', !kidRows[2]);
      var chip = q('.ca-helperschip', boss);
      var glyphs = chip ? all('.ca-helperschip-glyphs .ca-glyph', chip) : [];
      check('the session’s chip has each helper’s status, the one waiting on you first', glyphs.length === 3 && glyphs[0].classList.contains('ca-glyph--needs') && glyphs[1].classList.contains('ca-glyph--working'), glyphs.map(function (g) { return g.getAttribute('class'); }).join(' | '));
      check('in the attention color while one waits on you', !!chip && chip.classList.contains('is-waiting'));
      // A double-click lists them all under the session, the finished one too, and again lists
      // only the ones at work.
      chip.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      var finished = await until(function () { return rowOf(h.kids[2].title); }, 5000, 'the finished helper after a double-click');
      check('a double-click lists the finished one under it too', finished.classList.contains('is-nested') && q('.ca-helperschip', boss).classList.contains('is-shown'));
      check('and remembers it', (localStorage.getItem('codealong.helpersShown') || '').indexOf(h.boss.key) >= 0, localStorage.getItem('codealong.helpersShown'));
      q('.ca-helperschip', boss).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await until(function () { return !rowOf(h.kids[2].title); }, 5000, 'the finished helper gone again');
      check('another lists only the ones at work again', !q('.ca-helperschip', boss).classList.contains('is-shown') && !!rowOf(h.kids[0].title));

      // Resting on the chip brings up its card, which says what its tooltip would.
      var anchor = q('.ca-helperschip-anchor', boss);
      check('the chip leaves its tooltip to its card', !!anchor && !q('.ca-helperschip', boss).title);
      anchor.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      var card = await until(function () { return q('.ca-helperscard'); }, 3000, 'the helpers card');
      var text = function (el, sel) { var x = el && q(sel, el); return x ? x.textContent : ''; };
      var cardRows = all('.ca-helperscard-row', card);
      var kidBy = function (title) { return h.kids.filter(function (k) { return k.title === title; })[0]; };
      var chipBox = q('.ca-helperschip', boss).getBoundingClientRect();
      var cardBox = card.getBoundingClientRect();
      check('beside the chip, inside the window', (cardBox.top >= chipBox.bottom || cardBox.bottom <= chipBox.top) && cardBox.left >= 0 && cardBox.right <= innerWidth && cardBox.bottom <= innerHeight, JSON.stringify([Math.round(chipBox.bottom), Math.round(cardBox.top), Math.round(cardBox.bottom)]));
      check('its card names them', /^Its 3 helpers/.test(text(card, '.ca-hovercard-head')), text(card, '.ca-hovercard-head'));
      check('a row for each, the one waiting on you first, saying where it stands', cardRows.length === 3 && text(cardRows[0], '.ca-helperscard-name') === 'Check the migration on staging' && /^Waits on you, /.test(text(cardRows[0], '.ca-helperscard-stands')) && text(cardRows[1], '.ca-helperscard-name') === 'Review the diff for races' && /^Working for /.test(text(cardRows[1], '.ca-helperscard-stands')) && /^Done /.test(text(cardRows[2], '.ca-helperscard-stands')), cardRows.map(function (r) { return text(r, '.ca-helperscard-name') + ' / ' + text(r, '.ca-helperscard-stands'); }).join(' | '));
      check('and who runs it, naming its worktree only when it isn’t the session’s', cardRows.every(function (r) {
        var kid = kidBy(text(r, '.ca-helperscard-name'));
        var runs = text(r, '.ca-helperscard-runs');
        return !!kid && runs.indexOf(kid.account) === 0 && (runs.indexOf(' · in ') >= 0) === (kid.lane !== h.boss.lane);
      }), cardRows.map(function (r) { return text(r, '.ca-helperscard-runs'); }).join(' | '));
      check('with a way to list them all under it', /List them all under it/.test(text(card, '.ca-hovercard-foot')), text(card, '.ca-hovercard-foot'));
      // A right-click on a row is that helper's menu, not the session's.
      var menus = function () { return window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; }).length; };
      var before = menus();
      cardRows[2].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 }));
      await until(function () { return menus() > before; }, 3000, 'the helper’s menu');
      var items = window.__menu().map(function (i) { return i[0]; });
      check('a right-click on one is its menu', items.indexOf('Make it my own') >= 0, items.join(' | '));
      // A click on the finished one lists it under the session and shows it.
      var selectedBefore = q('.ca-sessionlist .is-selected');
      anchor.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      card = await until(function () { return q('.ca-helperscard'); }, 3000, 'the helpers card again');
      q('.ca-helperscard-main', all('.ca-helperscard-row', card)[2]).dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      var picked = await until(function () { var r = rowOf(h.kids[2].title); return r && r.classList.contains('is-selected') && r; }, 5000, 'the finished helper listed and selected after a click on it in the card');
      check('a click on one lists it under the session and selects it', picked.classList.contains('is-nested') && !q('.ca-helperscard') && !boss.classList.contains('is-selected'), picked.className + ' / was ' + (selectedBefore && selectedBefore.className));
      q('.ca-helperschip', boss).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await until(function () { return !rowOf(h.kids[2].title); }, 5000, 'the finished helper gone again');
      // Its arrow opens it where it runs: Codealong runs this one, so in its panel here, listed.
      anchor.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      card = await until(function () { return q('.ca-helperscard'); }, 3000, 'the helpers card once more');
      q('.ca-helperscard-open', all('.ca-helperscard-row', card)[2]).dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      var opened = await until(function () { var r = rowOf(h.kids[2].title); return r && r.classList.contains('is-selected') && r; }, 5000, 'the finished helper Codealong runs, opened from its arrow');
      check('its arrow opens one Codealong runs in its panel, listed under the session', opened.classList.contains('is-nested'));
      q('.ca-helperschip', boss).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await until(function () { return !rowOf(h.kids[2].title); }, 5000, 'the finished helper gone once more');
      // So too when a search lists it: opening it clears the search, and it stays listed.
      var search = q('input[placeholder^="Filter"]');
      type(search, 'zqhelpers');
      await until(function () { return rowOf(h.kids[2].title) && rowOf(h.boss.title); }, 5000, 'the session and its finished helper found by a search');
      boss = rowOf(h.boss.title);
      anchor = q('.ca-helperschip-anchor', boss);
      anchor.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      card = await until(function () { return q('.ca-helperscard'); }, 3000, 'the helpers card while searching');
      q('.ca-helperscard-open', all('.ca-helperscard-row', card)[2]).dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      var found = await until(function () { var r = rowOf(h.kids[2].title); return search.value === '' && r && r.classList.contains('is-selected') && r; }, 5000, 'the found helper, opened, with the search cleared');
      check('found by a search and opened from its arrow, it stays listed once the search clears', found.classList.contains('is-nested'));
      boss = rowOf(h.boss.title);
      q('.ca-helperschip', boss).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await until(function () { return !rowOf(h.kids[2].title); }, 5000, 'the finished helper gone after the search');
      // The search took the other rows away and drew them again.
      kidRows = h.kids.map(function (k) { return rowOf(k.title); });
      var section = boss.closest('section');
      check('and they’re in Needs you together', !!section && /Needs you/.test(section.textContent.slice(0, 40)) && kidRows[1].closest('section') === section, section && section.textContent.slice(0, 40));
      check('a nested helper has no helper chip of its own', !q('.ca-helperchip', kidRows[1]));

      await select(boss);
      await until(function () { return all('.ca-sessinsp-kids .ca-sessinsp-kin').length > 0; }, 10000, 'its panel’s helpers');
      // The latest of what came of it shows, the rest under "+N more": other sessions it
      // started may be later than its helpers.
      var kinMore = q('.ca-sessinsp-kids-more');
      if (kinMore && kinMore.getAttribute('aria-expanded') !== 'true') kinMore.click();
      var kids = await until(function () { var k = all('.ca-sessinsp-kids .ca-sessinsp-kin'); return (!kinMore || k.length > 1) && k; }, 5000, 'all of what came of it');
      check('its panel lists its helpers under its title', kids.some(function (k) { return /^Its helper /.test(k.title); }) && !!q('.ca-sessinsp-kids .ca-sessinsp-kin .ca-icon'), kids.map(function (k) { return k.title; }).join(' | '));

      await select(kidRows[1]);
      var parent = await until(function () { return q('.ca-sessinsp-kin.is-parent'); }, 10000, 'the helper’s panel');
      check('a helper’s panel names the session it helps', /^Helper of/.test(parent.textContent) && parent.textContent.indexOf(h.boss.title) >= 0, parent.textContent);
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
