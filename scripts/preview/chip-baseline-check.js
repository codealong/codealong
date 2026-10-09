// Checks that the words in a chip sit on one baseline at every zoom: a PR chip's number (mono),
// its state (sans) and its checks' count, an agent's account. Pieces of a chip each centered on
// their own land a pixel apart once the page is zoomed: the browser rounds each font's ascent
// and descent to whole device pixels, and the sans's and the mono's round differently (the
// 2026-10-09 report, at 110%). The page is zoomed with CSS `zoom`, as the floating windows'
// cards are (zoom.ts), which rounds as the main window's page zoom does. A text's baseline is
// its box's top plus its font's ascent as drawn, read off a probe in that font. The session
// shown has a PR, so its header shows PR chips whole, with their state.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh sessions '' scripts/preview/chip-baseline-check.js
(function () {
  var CHIPS = '.ca-chip, .ca-pr, .ca-labelchip, .ca-tag, .ca-pill';
  var ZOOMS = [1, 1.1, 1.25];
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
  // Each run of text in the chip, with the height its baseline is drawn at.
  var baselines = function (chip) {
    var out = [];
    var walk = document.createTreeWalker(chip, NodeFilter.SHOW_TEXT);
    for (var t = walk.nextNode(); t; t = walk.nextNode()) {
      if (!t.data.trim()) continue;
      var r = document.createRange();
      r.selectNodeContents(t);
      var box = r.getBoundingClientRect();
      if (box.width === 0) continue;
      out.push({ text: t.data.trim(), y: box.top + ascent(t.parentElement) });
    }
    return out;
  };
  var visible = function () {
    return Array.prototype.filter.call(document.querySelectorAll(CHIPS), function (c) {
      return c.getBoundingClientRect().width > 0 && !c.parentElement.closest(CHIPS);
    });
  };

  say('chip baseline check: waiting for a PR chip');
  until(function () { return document.querySelector('[role="option"] .ca-pr'); }, 20000, 'a row with a PR chip')
    .then(function (chip) {
      var row = chip.closest('[role="option"]');
      row.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return until(function () { return document.querySelector('.ca-pr .ca-pr-word'); }, 5000, "a PR chip with its state, in the selected row's header");
    })
    .then(function () { return document.fonts.ready; })
    .then(function () {
      var words = 0;
      ZOOMS.forEach(function (z) {
        document.documentElement.style.zoom = String(z);
        ascents = {};
        var off = [];
        var mixed = 0;
        visible().forEach(function (chip) {
          var b = baselines(chip);
          if (b.length < 2) return;
          mixed++;
          if (chip.querySelector('.ca-pr-word')) words++;
          var ys = b.map(function (x) { return x.y; });
          if (Math.max.apply(null, ys) - Math.min.apply(null, ys) > 0.1) off.push(b.map(function (x) { return x.text + '@' + x.y.toFixed(2); }).join(' '));
        });
        note(Math.round(z * 100) + '%: ' + mixed + ' chips with more than one run of text');
        check('at ' + Math.round(z * 100) + '% every chip puts its words on one baseline', off.length === 0, off.length + ' off: ' + off.slice(0, 4).join(' | '));
      });
      document.documentElement.style.zoom = '';
      check('PR chips with their state were measured', words > 0);
    })
    .then(done)
    .catch(stopped);
})();
