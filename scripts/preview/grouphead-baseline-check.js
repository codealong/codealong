// Checks that a group's heading (LaneRow.tsx `WorktreeHead`, `RepoHead`) sets its text on one
// baseline at every zoom: the project and the name (sans), where it is (mono), and at its end
// the diff and the drift (mono). Pieces each centered on their own put the mono a pixel above
// the sans (the 2026-11-06 report, at 110% on a Retina screen): the two fonts sit their text at
// different heights in a box of one height. The chips are boxes centered on the line, so their
// words are only held to a pixel of the name's. Zoomed with CSS `zoom` as chip-baseline-check.js
// is, at 200% and 220% too, which round as a Retina screen does at 100% and 110%. A text's
// baseline is its box's top plus its font's ascent as drawn, read off a probe in that font.
//   PREVIEW_GROUP=worktree scripts/preview.sh sessions '' scripts/preview/grouphead-baseline-check.js
//   PREVIEW_GROUP=worktree scripts/preview.sh prs '' scripts/preview/grouphead-baseline-check.js
(function () {
  var ZOOMS = [1, 1.1, 1.25, 2, 2.2];
  var ascents = {};
  // How far above the baseline a font's text box starts, as this page draws it now.
  var ascent = function (el) {
    var cs = getComputedStyle(el);
    var font = cs.fontStyle + ' ' + cs.fontWeight + ' ' + cs.fontSize + ' ' + cs.fontFamily;
    if (ascents[font] === undefined) {
      var probe = document.createElement('div');
      probe.style.cssText = 'position:absolute;left:0;top:0;visibility:hidden;white-space:nowrap;line-height:normal';
      probe.style.font = font;
      probe.textContent = 'x';
      var mark = document.createElement('span');
      mark.style.cssText = 'display:inline-block;width:0;height:0';
      probe.appendChild(mark);
      document.body.appendChild(probe);
      var r = document.createRange();
      r.selectNodeContents(probe.firstChild);
      ascents[font] = mark.getBoundingClientRect().bottom - r.getBoundingClientRect().top;
      probe.remove();
    }
    return ascents[font];
  };
  // Each run of text under `root` that shows, with the height its baseline is drawn at.
  var baselines = function (root) {
    var out = [];
    var walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (var t = walk.nextNode(); t; t = walk.nextNode()) {
      if (!t.data.trim()) continue;
      var r = document.createRange();
      r.selectNodeContents(t);
      var box = r.getBoundingClientRect();
      if (box.width === 0) continue;
      // A chip that didn't fit wrapped onto the lane's second line, which is cut off.
      var lane = t.parentElement.closest('.ca-grouphead-chips');
      if (lane && box.top >= lane.getBoundingClientRect().bottom) continue;
      out.push({ text: t.data.trim().slice(0, 12), y: box.top + ascent(t.parentElement), chip: !!lane });
    }
    return out;
  };
  var said = function (b) {
    return b.map(function (x) { return x.text + '@' + x.y.toFixed(2); }).join(' ');
  };

  say('group heading baseline check: waiting for a worktree heading');
  until(function () { return document.querySelector('.ca-grouphead.is-worktree-head .ca-grouphead-sub'); }, 20000, "a worktree's heading (PREVIEW_GROUP=worktree)")
    .then(function () { return document.fonts.ready; })
    .then(function () {
      var mixed = 0;
      var chipped = 0;
      ZOOMS.forEach(function (z) {
        document.documentElement.style.zoom = String(z);
        ascents = {};
        var off = [];
        var chipsOff = [];
        Array.prototype.forEach.call(document.querySelectorAll('.ca-grouphead-line'), function (line) {
          if (line.getBoundingClientRect().width === 0) return;
          var b = baselines(line);
          var text = b.filter(function (x) { return !x.chip; });
          var name = line.querySelector('.ca-grouphead-name');
          if (text.length < 2 || !name) return;
          if (z === 1) mixed++;
          var ys = text.map(function (x) { return x.y; });
          if (Math.max.apply(null, ys) - Math.min.apply(null, ys) > 0.1) off.push(said(text));
          var at = baselines(name)[0];
          b.filter(function (x) { return x.chip; }).forEach(function (x) {
            if (z === 1) chipped++;
            if (at && Math.abs(x.y - at.y) > 1) chipsOff.push(x.text + '@' + x.y.toFixed(2) + ' name@' + at.y.toFixed(2));
          });
        });
        var pct = Math.round(z * 100) + '%';
        check('at ' + pct + ' every heading sets its name, where it is and its diff on one baseline', off.length === 0, off.length + ' off: ' + off.slice(0, 3).join(' | '));
        check('at ' + pct + " every heading's chips say their words within a pixel of its name's baseline", chipsOff.length === 0, chipsOff.length + ' off: ' + chipsOff.slice(0, 3).join(' | '));
      });
      document.documentElement.style.zoom = '';
      note(mixed + ' headings with more than one run of text, ' + chipped + ' words in their chips');
      check('headings with mono and sans text were measured', mixed > 0);
    })
    .then(done)
    .catch(stopped);
})();
