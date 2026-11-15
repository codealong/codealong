// Checks glass (`[appearance] glass`, src/glass.ts) as the pages draw it. In the main window the
// root says is-glass and draws no ground of its own, and the board's ground is frosted, not clear;
// what sits on the glass (the title bar, the usage bar, the sidebar, the strip over the list, the
// group headers) draws its wells, selections and lines see-through, so they take the glass's tint
// (styles/glass.css), while surface-hover stays opaque there, so a white control keeps its white on
// hover and a usage chip while it's dragged over the others. In the quick panel the bar along the
// card's foot is see-through and the card above it isn't. On Linux there's no glass to let through,
// so neither page is see-through anywhere.
//   PREVIEW_GLASS=1 scripts/preview.sh sessions '' scripts/preview/glass-check.js
//   PREVIEW_GLASS=1 PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/glass-check.js
//   PREVIEW_WAIT=60000 PREVIEW_GLASS=1 scripts/preview.sh quick '' scripts/preview/glass-check.js
//   PREVIEW_GLASS=1 PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/glass-check.js
(function () {
  var linux = window.__CODEALONG_PLATFORM__ === 'linux';
  // The quick panel's page, as its window says (quick.rs `setup`).
  var quick = !!window.__CODEALONG_QUICK__;
  // A color's opacity, as computed (rgb(), rgba()) or as the build writes a token (#rgb, #rrggbb,
  // and #rrggbbaa for one with alpha).
  var alpha = function (color) {
    var c = String(color).trim();
    var hex = c.match(/^#([0-9a-f]{3,8})$/i);
    if (hex) return hex[1].length === 8 ? parseInt(hex[1].slice(6), 16) / 255 : hex[1].length === 4 ? parseInt(hex[1][3] + hex[1][3], 16) / 255 : 1;
    var m = c.match(/rgba?\(([^)]+)\)/);
    if (!m) return NaN;
    var p = m[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return p.length > 3 ? p[3] : 1;
  };
  var bg = function (el) { return getComputedStyle(el).backgroundColor; };
  var token = function (el, name) { return getComputedStyle(el).getPropertyValue(name).trim(); };
  var run = async function () {
    if (quick) {
      var bar = await until(function () { return q('.ca-quick-bar'); }, 20000, 'the quick panel’s bar');
      var card = q('.ca-quick');
      if (linux) {
        check('the bar keeps its ground', alpha(bg(bar)) === 1, bg(bar));
        return;
      }
      // It says so once the board comes, after the card is drawn.
      await until(function () { return document.documentElement.classList.contains('is-glass'); }, 20000, 'is-glass on the quick panel’s page');
      check('the bar lets the glass through', alpha(bg(bar)) < 0.5, bg(bar));
      check('the card above it keeps its ground', alpha(bg(card)) === 1, bg(card));
      check('a chip on the bar keeps an opaque hover', alpha(token(bar, '--surface-hover')) === 1, token(bar, '--surface-hover'));
      return;
    }
    await until(function () { return q('.ca-usagebar .ca-uchip') && q('.ca-side') && q('.ca-listbar') && q('.ca-bay'); }, 20000, 'the usage bar, the sidebar, the strip over the list and a group header');
    var root = document.documentElement;
    var board = q('.ca-board');
    if (linux) {
      check('Linux has no glass: the page doesn’t say is-glass', !root.classList.contains('is-glass'), root.className);
      check('and the board keeps its ground', alpha(bg(board)) === 1, bg(board));
      return;
    }
    check('the page says is-glass', root.classList.contains('is-glass'), root.className);
    check('the page draws no ground of its own', alpha(bg(document.body)) === 0 && alpha(bg(q('.ca-win'))) === 0, bg(document.body) + ' | ' + bg(q('.ca-win')));
    var frost = alpha(bg(board));
    check('the board’s ground is frosted: mostly opaque, not clear', frost > 0.6 && frost < 1, bg(board));
    ['.ca-win-bar', '.ca-usagebar', '.ca-side', '.ca-listbar', '.ca-bay'].forEach(function (sel) {
      var el = q(sel);
      var see = ['--surface-sunken', '--surface-selected', '--line', '--neutral-tint'].filter(function (t) { return !(alpha(token(el, t)) < 1); });
      check(sel + ' draws its wells, selections, lines and keycaps see-through', see.length === 0, see.map(function (t) { return t + ' ' + token(el, t); }).join(', '));
      check(sel + ' keeps surface-hover opaque', alpha(token(el, '--surface-hover')) === 1, token(el, '--surface-hover'));
    });
    var chip = q('.ca-usagebar .ca-uchip');
    check('a usage chip keeps its white', alpha(bg(chip)) === 1, bg(chip));
    var row = q('.ca-board .ca-lane');
    if (row) check('a row keeps its ground', alpha(bg(row)) === 1, bg(row));
    else note('no row to look at');
  };
  run().then(done).catch(stopped);
})();
