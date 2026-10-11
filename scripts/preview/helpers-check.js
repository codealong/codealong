// Checks helpers in Sessions (SessionsView.tsx `sessionSections`, boardLists.ts `placeHelpers`),
// on the board helpers-init.js makes: the helpers that work or wait on you are listed under the
// session that started them, set in, each on one line with no divider above it and a thread from
// the session's status into its own (LaneRow.css), and the finished one isn't; the session's row
// says one of its helpers waits on you, and the helper that does takes the session into Needs you
// with it. The session's panel lists its helpers under its title, and a helper's panel names the
// session as the one it helps.
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
      check('the session’s row says a helper waits on you', !!chip && chip.classList.contains('is-waiting') && /1 waiting/.test(chip.textContent), chip && chip.textContent);
      check('which its tooltip counts', !!chip && /3 helpers .*1 working, 1 waiting on you/.test(chip.title), chip && chip.title);
      var section = boss.closest('section');
      check('and they’re in Needs you together', !!section && /Needs you/.test(section.textContent.slice(0, 40)) && kidRows[1].closest('section') === section, section && section.textContent.slice(0, 40));
      check('a nested helper has no helper chip of its own', !q('.ca-helperchip', kidRows[1]));

      await select(boss);
      var kids = await until(function () { return all('.ca-sessinsp-kids .ca-sessinsp-kin').length > 0 && all('.ca-sessinsp-kids .ca-sessinsp-kin'); }, 10000, 'its panel’s helpers');
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
