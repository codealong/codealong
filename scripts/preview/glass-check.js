// Checks glass (`[appearance] glass`, src/glass.ts) as the pages draw it. In the main window the
// root says is-glass and draws no ground of its own, the window a see-through tint that the title
// bar, the sidebar and the usage bar draw nothing over (one pane of glass), and the board's ground
// is frosted, not clear;
// what sits on the glass (the title bar, the usage bar, the sidebar, the strip over the list, the
// group headers) draws its wells, hovers, selections and lines see-through, so they take the
// glass's tint (styles/glass.css). Controls on live window glass (title bar buttons, the luminous
// frosted search pill, segmented controls, sidebar active items, quick panel chips, and usage chips
// on hover) draw as translucent glass pills with no opaque white boxes or palette-tinted borders,
// while a dragged usage chip keeps its opaque ground so chips underneath don't bleed through; the
// strip over the list hides the rows that scroll under it, the strip over a session's panel has
// its ground and line, and nothing draws a blur of its own (WebKit's, stacked on the glass,
// flickers as the window moves). In the quick panel the card draws no ground of its own, which
// would cover the glass under it, its parts above the bar draw it, the bar is see-through, its
// chips are translucent glass pills, and in Send to, which has no bar, the part at the foot has
// the card's corners. On Linux there's no glass to let through, so neither page is see-through
// anywhere.
//   PREVIEW_GLASS=1 scripts/preview.sh sessions '' scripts/preview/glass-check.js
//   PREVIEW_GLASS=1 PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/glass-check.js
//   PREVIEW_WAIT=60000 PREVIEW_GLASS=1 scripts/preview.sh quick '' scripts/preview/glass-check.js
//   PREVIEW_GLASS=1 PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/glass-check.js
(function () {
  var linux = window.__CODEALONG_PLATFORM__ === 'linux';
  // The quick panel's page, as its window says (quick.rs `setup`).
  var quick = !!window.__CODEALONG_QUICK__;
  // Before the app's script: every project's sessions, for Send to to list (quick-sendto-check.js).
  if (quick) localStorage.setItem('codealong.quick.project', 'All projects');
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
  // The background a control's :hover rule gives it, drawn on a probe in `place` (a :hover can't be
  // made to happen here): its color, under whatever it lays over it.
  var rules = function () { return [].concat.apply([], Array.prototype.map.call(document.styleSheets, function (sh) { try { return Array.prototype.slice.call(sh.cssRules); } catch (e) { return []; } })); };
  var hoverOn = function (selector, place) {
    var rule = rules().filter(function (r) { return r.selectorText && r.selectorText.split(',').some(function (s) { return s.trim() === selector; }); })[0];
    if (!rule) return null;
    var probe = document.createElement('div');
    place.appendChild(probe);
    if (rule.style.background) probe.style.background = rule.style.background;
    else probe.style.backgroundColor = rule.style.backgroundColor;
    var color = bg(probe);
    probe.remove();
    return color;
  };
  var hovers = function (selector, place, opaque, what) {
    var color = hoverOn(selector, place);
    check(what, !!color && (opaque ? alpha(color) === 1 : alpha(color) > 0 && alpha(color) < 1), selector + ' → ' + (color || 'no rule for it'));
  };
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
      check('the card draws no ground under it', alpha(bg(card)) === 0, bg(card));
      var parts = Array.prototype.filter.call(card.children, function (c) { return c !== bar && !c.classList.contains('ca-scrim'); });
      var clear = parts.filter(function (c) { return alpha(bg(c)) !== 1; });
      check('each part above the bar draws the card’s ground', parts.length > 0 && clear.length === 0, clear.map(function (c) { return c.className + ' ' + bg(c); }).join(', '));
      var qchip = bar.querySelector('.ca-quick-chip');
      check('a chip on the bar is a translucent glass pill', !!qchip && alpha(bg(qchip)) > 0 && alpha(bg(qchip)) < 1 && parseFloat(getComputedStyle(qchip).borderTopLeftRadius) >= 100, qchip ? bg(qchip) + ' r=' + getComputedStyle(qchip).borderTopLeftRadius : 'none');
      hovers('.is-glass .ca-quick-bar .ca-quick-chip:hover:not(:disabled)', bar, false, 'a chip on the bar stays translucent glass under the pointer');
      // Send to (⌘F under a prompt) has no bar: the part at the card's foot has its corners.
      var box = q('.ca-quick-input textarea');
      type(box, 'Look at why the build fails on main and fix it');
      for (var i = 0; i < 20 && q('.ca-quick-bar'); i++) {
        await wait(250);
        key(box, { key: 'f', metaKey: true });
      }
      await wait(300);
      if (q('.ca-quick-bar')) {
        // Nothing here it can send to: the bar is taken out as Send to leaves it out.
        note('Send to didn’t open, with nothing here to send to: the bar is taken out as it leaves it out');
        q('.ca-quick-bar').remove();
      }
      var foot = Array.prototype.filter.call(card.children, function (c) { return !c.classList.contains('ca-scrim'); }).pop();
      var corners = getComputedStyle(foot);
      check('in Send to, the part at the card’s foot has its bottom corners', corners.borderBottomLeftRadius === '12px' && corners.borderBottomRightRadius === '12px', foot.className + ' ' + corners.borderBottomLeftRadius + ' ' + corners.borderBottomRightRadius);
      return;
    }
    await until(function () { return q('.ca-statusbar .ca-uchip') && q('.ca-side') && q('.ca-listbar') && q('.ca-bay'); }, 20000, 'the status bar, the sidebar, the strip over the list and a group header');
    var root = document.documentElement;
    var board = q('.ca-board');
    if (linux) {
      check('Linux has no glass: the page doesn’t say is-glass', !root.classList.contains('is-glass'), root.className);
      check('and the board keeps its ground', alpha(bg(board)) === 1, bg(board));
      return;
    }
    check('the page says is-glass', root.classList.contains('is-glass'), root.className);
    check('the page draws no ground of its own, the window only a see-through tint', alpha(bg(document.body)) === 0 && alpha(bg(q('.ca-win'))) < 0.5, bg(document.body) + ' | ' + bg(q('.ca-win')));
    var over = ['.ca-win-bar', '.ca-statusbar', '.ca-side'].filter(function (sel) { return alpha(bg(q(sel))) !== 0; });
    check('the title bar, the status bar and the sidebar are one pane: none draws over the window’s tint', over.length === 0, over.map(function (sel) { return sel + ' ' + bg(q(sel)); }).join(', '));
    var frost = alpha(bg(board));
    check('the board’s ground is frosted: mostly opaque, not clear', frost > 0.6 && frost < 1, bg(board));
    ['.ca-win-bar', '.ca-statusbar', '.ca-side', '.ca-listbar', '.ca-bay'].forEach(function (sel) {
      var el = q(sel);
      var see = ['--surface-sunken', '--surface-hover', '--surface-selected', '--line', '--line-strong', '--neutral-tint', '--ink-2', '--ink-3'].filter(function (t) { return !(alpha(token(el, t)) < 1); });
      check(sel + ' draws its wells, hovers, selections, lines, keycaps and muted ink see-through', see.length === 0, see.map(function (t) { return t + ' ' + token(el, t); }).join(', '));
    });
    var statusBar = q('.ca-statusbar');
    check('secondary and muted ink on the glass keep strong contrast alpha', alpha(token(statusBar, '--ink-2')) >= 0.7 && alpha(token(statusBar, '--ink-3')) >= 0.6, token(statusBar, '--ink-2') + ' | ' + token(statusBar, '--ink-3'));
    check('meter tracks on the glass stay visible', alpha(token(statusBar, '--neutral-tint')) >= 0.1, token(statusBar, '--neutral-tint'));
    check('the status bar has the title bar’s glass', bg(statusBar) === bg(q('.ca-win-bar')), bg(statusBar) + ' | ' + bg(q('.ca-win-bar')));
    var strip = q('.ca-listbar');
    check('the strip over the list hides the rows that scroll under it: opaque', alpha(bg(strip)) === 1, bg(strip));
    // The strip over a session's panel is the list's strip carried on across the window: its shade,
    // and the same see-through shades on it.
    var panelStrip = q('.ca-sessinsp .ca-reply-queue');
    if (panelStrip) {
      check('the strip over a session’s panel has the list strip’s ground', bg(panelStrip) === bg(strip), bg(panelStrip) + ' | ' + bg(strip));
      check('and its line', getComputedStyle(panelStrip).borderBottomColor === getComputedStyle(strip).borderBottomColor, getComputedStyle(panelStrip).borderBottomColor + ' | ' + getComputedStyle(strip).borderBottomColor);
    } else note('no session’s panel to look at');
    // A page's own blur, stacked on the glass behind the window, flickers as the window moves.
    var blurred = all('body *').filter(function (el) { var cs = getComputedStyle(el); return (cs.backdropFilter || cs.webkitBackdropFilter || 'none') !== 'none'; });
    check('nothing on the page draws a blur of its own over the glass', blurred.length === 0, blurred.slice(0, 3).map(function (el) { return el.className; }).join(', '));
    // Controls on live window glass draw as thin translucent capsules (no gloss gradient sheen);
    // hovers and sidebar selections are plain translucent tints, while a dragged usage chip stays
    // opaque so sliding chips underneath don't bleed through.
    var winBar = q('.ca-win-bar');
    var platter = q('.ca-win-bar-end > .ca-win-bar-group');
    var search = q('.ca-win-bar .ca-search');
    var seg = q('.ca-win-bar-end > .ca-segctl');
    check('title bar platters are thin translucent glass capsules with no gloss gradient', !!platter && alpha(bg(platter)) > 0 && alpha(bg(platter)) < 1 && getComputedStyle(platter).backgroundImage === 'none' && parseFloat(getComputedStyle(platter).borderTopLeftRadius) >= 100, platter ? bg(platter) + ' img=' + getComputedStyle(platter).backgroundImage + ' r=' + getComputedStyle(platter).borderTopLeftRadius : 'none');
    check('the search pill is frosted glass with plain shortcut text', !!search && alpha(bg(search)) > 0 && alpha(bg(search)) < 1 && getComputedStyle(search).backgroundImage === 'none' && parseFloat(getComputedStyle(search).borderTopLeftRadius) >= 100, search ? bg(search) + ' r=' + getComputedStyle(search).borderTopLeftRadius : 'none');
    var searchKbd = search && search.querySelector('.ca-kbd');
    if (searchKbd) check('search pill shortcut has no pill box inside the pill', alpha(bg(searchKbd)) === 0 && getComputedStyle(searchKbd).borderTopWidth === '0px', bg(searchKbd) + ' b=' + getComputedStyle(searchKbd).borderTopWidth);
    check('the title bar segmented control matches the toolbar platter’s fill and height', !!seg && !!platter && bg(seg) === bg(platter) && parseFloat(getComputedStyle(seg).borderTopLeftRadius) >= 100 && Math.abs(seg.getBoundingClientRect().height - platter.getBoundingClientRect().height) <= 0.5, seg && platter ? bg(seg) + ' h=' + seg.getBoundingClientRect().height + ' vs ' + platter.getBoundingClientRect().height : 'none');
    var navActive = q('.ca-side .ca-nav.is-active');
    if (navActive) check('the active sidebar row is a flat translucent tint', alpha(bg(navActive)) > 0 && alpha(bg(navActive)) < 1 && getComputedStyle(navActive).boxShadow === 'none', bg(navActive) + ' shadow=' + getComputedStyle(navActive).boxShadow);
    hovers('.is-glass .ca-win-bar .ca-search:hover', winBar, false, 'the search pill’s hover is translucent glass');
    hovers('.is-glass .ca-win-bar .ca-sidetoggle:hover', winBar, false, 'the sidebar toggle’s hover is a translucent tint');
    hovers('.is-glass .ca-win-bar-end > .ca-win-bar-group > .ca-btn:hover:not(:disabled)', winBar, false, 'a toolbar platter button’s hover is a translucent tint');
    hovers('.is-glass .ca-statusbar .ca-uchip:hover:not(.is-lifted)', statusBar, false, 'a usage chip’s hover is a flat translucent tint');
    hovers('.is-glass .ca-statusbar .ca-statusbar-accts.is-sorting .ca-uchip.is-lifted', statusBar, true, 'a dragged usage chip stays opaque over the others');
    // At rest a usage chip is flat, as a status bar's items are: the glass shows through it.
    var chip = q('.ca-statusbar .ca-uchip:not(.is-attn):not(.is-training)');
    if (chip) check('a usage chip at rest draws no ground of its own', alpha(bg(chip)) === 0, bg(chip));
    else note('no usage chip at rest to look at');
    var row = q('.ca-board .ca-lane');
    if (row) check('a row keeps its ground', alpha(bg(row)) === 1, bg(row));
    else note('no row to look at');
  };
  run().then(done).catch(stopped);
})();
