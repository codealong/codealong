// Checks the main window's chrome (App.css): the title bar, the sidebar and the status bar are one
// ground, the window's, with no line between them; the board and the panel beside it are a card on
// it, rounded at its four corners, with a gutter at the window's right edge and a shadow under it
// on the window's ground, beneath the chrome's rows. Settings, Analytics and the GitHub link are
// icons at the sidebar's foot, named for the keyboard and screen readers, with the theme switch
// after them, and the rail the sidebar folds to has the three; the status bar ends with Refresh,
// with no Accounts; a chip names a model's own limit only beside its others, and says when a
// limit resets only once it's over 70%, a flagged chip's card naming them all. A click on the
// GitHub link opens Codealong's repo, and one on Analytics opens its window. While the board is
// read there's no panel, and the board alone rounds all four corners. At the window's least width
// (980) the grid and its gutter fit, and the status bar's end keeps Refresh in the window beside
// notices, a meeting and the machine's load.
//   scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_GLASS=1 scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_USAGE=scripts/preview/usage-sample.json scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_SIZE=2600,900 PREVIEW_USAGE=scripts/preview/usage-sample.json PREVIEW_TRAINING=scripts/preview/training-sample.json scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_LOADING=1 scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_SIZE=980,700 PREVIEW_NOTICES=scripts/preview/notices-sample.json PREVIEW_INIT=scripts/preview/window-chrome-crowded-init.js scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
(function () {
  var REPO = 'https://github.com/codealong/codealong';
  var style = function (el) { return getComputedStyle(el); };
  var clear = function (el) { var b = style(el).backgroundColor; return b === 'rgba(0, 0, 0, 0)' || b === 'transparent'; };
  var rounded = function (el, corners) { return corners.every(function (c) { return parseFloat(style(el)['border' + c + 'Radius']) > 0; }); };
  var shadow = function () { return getComputedStyle(q('.ca-win-body'), '::before'); };
  // The board as the card alone, while the board is being read.
  var alone = async function () {
    var board = await until(function () { return q('.ca-win-body.is-loading > .ca-board'); }, 20000, 'the board, reading');
    check('nothing comes after the board while it reads', board.parentElement.lastElementChild === board, board.parentElement.lastElementChild.className);
    check('the board alone rounds all four corners', rounded(board, ['TopLeft', 'TopRight', 'BottomLeft', 'BottomRight']), ['TopLeft', 'TopRight', 'BottomRight', 'BottomLeft'].map(function (c) { return style(board)['border' + c + 'Radius']; }).join(' '));
    check('and draws its hairline on the right too', style(board).borderRightWidth === '1px', style(board).borderRightWidth);
    check('its clip-path rounds them', /round/.test(style(board).clipPath), style(board).clipPath);
    var edge = window.innerWidth - board.getBoundingClientRect().right;
    check('with the gutter at the window’s right edge', edge >= 4 && edge <= 12, Math.round(edge) + 'px');
    check('its shadow is the board’s width', Math.abs(parseFloat(shadow().width) - board.getBoundingClientRect().width) <= 1, shadow().width + ' for ' + board.getBoundingClientRect().width);
    done();
  };
  // The window at its least width: everything in it.
  var narrow = function () {
    var body = q('.ca-win-body');
    var kids = Array.prototype.filter.call(body.children, function (c) { return c.getBoundingClientRect().width > 0; });
    var right = Math.max.apply(null, kids.map(function (c) { return c.getBoundingClientRect().right; }));
    check('at its least width the board, the panel and the gutter fit the window', body.scrollWidth <= body.clientWidth && right <= window.innerWidth - 4, 'children reach ' + Math.round(right) + ' of ' + window.innerWidth + ', scroll ' + body.scrollWidth + ' of ' + body.clientWidth);
    var refresh = q('.ca-usagebar-end button[aria-label="Refresh usage"]').getBoundingClientRect();
    // Crowded (window-chrome-crowded-init.js): the notices and a meeting with a long name.
    var meeting = q('.ca-usagebar-end .ca-meeting.is-line');
    check('the bar is crowded: notices and a meeting beside Refresh', !!q('.ca-usagebar-end .ca-notice') && !!meeting, 'notice ' + !!q('.ca-usagebar-end .ca-notice') + ', meeting ' + !!meeting);
    check('and Refresh stays in the window', refresh.right <= window.innerWidth && refresh.left >= q('.ca-usagebar').getBoundingClientRect().left, Math.round(refresh.left) + '–' + Math.round(refresh.right) + ' of ' + window.innerWidth);
    // What's cut is cut at the left; nothing it shows spills over what's beside it.
    var hits = all('.ca-usagebar-end button').filter(function (b) {
      var r = b.getBoundingClientRect();
      var x = r.left + r.width / 2;
      if (x < q('.ca-usagebar-end').getBoundingClientRect().left) return false;
      var at = document.elementFromPoint(x, r.top + r.height / 2);
      return !(at && b.contains(at));
    });
    check('each button it shows is the one under its middle', hits.length === 0, hits.map(function (b) { return b.textContent.trim() || b.getAttribute('aria-label'); }).join(', '));
  };
  var run = async function () {
    if (q('.ca-win-body.is-loading') || window.__PREVIEW_LOADING__) return alone();
    // The window lays itself out again once the board is read, so it's looked at after that.
    await until(function () { return q('.ca-sessionlist .ca-lane') && q('.ca-sidetoggle'); }, 20000, 'the sessions');
    await wait(300);
    if (!q('.ca-side')) q('.ca-sidetoggle').click();
    await until(function () { return q('.ca-side'); }, 3000, 'the sidebar open');

    var win = q('.ca-win--app');
    var bar = q('.ca-win-bar');
    var side = q('.ca-side');
    var foot = q('.ca-usagebar');
    check('the window has a ground of its own for the chrome', !clear(win) || document.documentElement.classList.contains('is-glass'), style(win).backgroundColor);
    check('the title bar, the sidebar and the status bar draw none over it', clear(bar) && clear(side) && clear(foot), [bar, side, foot].map(function (el) { return style(el).backgroundColor; }).join(' | '));
    check('and no line between them', style(bar).borderBottomWidth === '0px' && style(side).borderRightWidth === '0px' && style(foot).borderTopWidth === '0px', [style(bar).borderBottomWidth, style(side).borderRightWidth, style(foot).borderTopWidth].join(' | '));

    // A session picked, so the panel is beside the board.
    var lane = q('.ca-sessionlist .ca-lane');
    lane.click();
    var panel = await until(function () { return q('.ca-win-body > .ca-insp'); }, 5000, 'the panel');
    var board = q('.ca-board');
    var b = style(board);
    var p = style(panel);
    check('the board rounds the card’s left corners', parseFloat(b.borderTopLeftRadius) > 0 && parseFloat(b.borderBottomLeftRadius) > 0 && parseFloat(b.borderTopRightRadius) === 0, [b.borderTopLeftRadius, b.borderTopRightRadius, b.borderBottomLeftRadius].join(' '));
    check('and the panel its right ones', parseFloat(p.borderTopRightRadius) > 0 && parseFloat(p.borderBottomRightRadius) > 0 && parseFloat(p.borderTopLeftRadius) === 0, [p.borderTopLeftRadius, p.borderTopRightRadius, p.borderBottomRightRadius].join(' '));
    var edge = window.innerWidth - panel.getBoundingClientRect().right;
    check('with a gutter at the window’s right edge', edge >= 4 && edge <= 12, Math.round(edge) + 'px');
    check('the card meets the title bar and the status bar', Math.abs(board.getBoundingClientRect().top - bar.getBoundingClientRect().bottom) <= 1 && Math.abs(board.getBoundingClientRect().bottom - foot.getBoundingClientRect().top) <= 1, JSON.stringify({ board: board.getBoundingClientRect(), bar: bar.getBoundingClientRect().bottom, foot: foot.getBoundingClientRect().top }));
    // The card's shadow: the board and the panel's width together, under the window's rows.
    var s = shadow();
    var cardWidth = panel.getBoundingClientRect().right - board.getBoundingClientRect().left;
    check('the card casts a shadow', s.content !== 'none' && s.boxShadow !== 'none' && /\d/.test(s.boxShadow), s.content + ' ' + s.boxShadow);
    check('as wide as the board and the panel together', Math.abs(parseFloat(s.width) - cardWidth) <= 1 && Math.abs(parseFloat(s.height) - board.getBoundingClientRect().height) <= 1, s.width + ' × ' + s.height + ' for ' + cardWidth);
    check('it takes no clicks', s.pointerEvents === 'none', s.pointerEvents);
    // The window isn't a stacking context: one would hold its dialogs under what's portalled to the page.
    check('and the window stays no stacking context of its own', style(win).isolation !== 'isolate' && style(win).zIndex === 'auto' && style(win).transform === 'none', style(win).isolation + ' ' + style(win).zIndex);
    // The shadow is first in the body; the sidebar and the status bar, positioned, paint after it.
    check('the sidebar and the status bar paint over the shadow', style(side).position === 'relative' && style(foot).position === 'relative', style(side).position + ' ' + style(foot).position);

    if (window.innerWidth <= 1000) narrow();
    var end = q('.ca-usagebar-end');
    check('the status bar ends with Refresh', !!end.lastElementChild && end.lastElementChild.getAttribute('aria-label') === 'Refresh usage', end.lastElementChild && end.lastElementChild.outerHTML.slice(0, 120));
    check('with no Accounts, Analytics, Settings or theme there', !end.querySelector('[aria-label="Accounts"], [aria-label="Analytics"], [aria-label="Settings"], .ca-theme'), end.innerHTML.slice(0, 200));

    // A model's own limit is named (its sparkle and name) only beside the chip's other limits.
    var lone = 0, beside = 0;
    var misnamed = all('.ca-usagebar-accts .ca-uchip').filter(function (c) {
      var ms = Array.prototype.slice.call(c.querySelectorAll('.ca-meter'));
      return ms.some(function (m) {
        var label = m.querySelector('.ca-meter-label');
        if (!m.classList.contains('is-model')) return !label;
        if (ms.length === 1) { lone++; return !!label; }
        beside++;
        return !label || !label.querySelector('svg');
      });
    });
    note(lone + ' model limits alone in their chip, ' + beside + ' beside others');
    check('a model’s limit alone in its chip goes unnamed, and is named beside others', misnamed.length === 0, misnamed.map(function (c) { return c.getAttribute('aria-label'); }).join(' | '));
    // When a limit resets shows only once it's over 70% (UsageBar.tsx RESET_SHOWN_ABOVE).
    var meters = all('.ca-usagebar-accts .ca-uchip .ca-meter');
    var full = meters.filter(function (m) { return parseFloat(m.querySelector('.ca-meter-value').textContent) > 70; });
    var resets = function (m) { var r = m.querySelector('.ca-meter-reset'); return !!r && !!r.textContent.trim(); };
    var early = meters.filter(function (m) { return resets(m) && parseFloat(m.querySelector('.ca-meter-value').textContent) <= 70; });
    // One over 70% with a reset to tell (its chip's label says so) shows it, but in a tight chip.
    var due = full.filter(function (m) {
      var chip = m.closest('.ca-uchip');
      return !chip.classList.contains('is-tight') && (chip.getAttribute('aria-label') || '').indexOf(m.querySelector('.ca-meter-value').textContent + ' used, resets') >= 0;
    });
    var missing = due.filter(function (m) { return !resets(m); });
    note(meters.length + ' limits in the chips, ' + full.length + ' over 70%, ' + due.length + ' with a reset to show');
    check('a limit says when it resets only once it’s over 70%', early.length === 0, early.map(function (m) { return m.textContent.trim(); }).join(' | '));
    check('and then it does', missing.length === 0, missing.map(function (m) { return m.textContent.trim(); }).join(' | '));
    // A chip flagged for training has a card in place of its tooltip, so the card names its limits
    // and says when they reset, though the chip shows them all.
    var flagged = q('.ca-usagebar-accts .ca-uchip.is-training');
    if (flagged && flagged.querySelector('.ca-meter')) {
      flagged.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body }));
      var trainingCard = await until(function () { return q('.ca-usage-popover.is-training'); }, 3000, 'the flagged chip’s card');
      check('a flagged chip’s card lists its limits', !!trainingCard.querySelector('.ca-ucard-meters .ca-meter'), trainingCard.textContent.slice(0, 120));
      flagged.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body }));
      await until(function () { return !q('.ca-usage-popover'); }, 3000, 'the card gone');
    } else note('no flagged chip with limits to look at');

    var feet = q('.ca-side .ca-side-foot');
    var gear = feet && feet.querySelector('.ca-side-config');
    var chart = feet && feet.querySelector('.ca-side-analytics');
    var github = feet && feet.querySelector('.ca-side-github');
    var theme = feet && feet.querySelector('.ca-theme');
    check('Settings, Analytics, the GitHub link and the theme are at the sidebar’s foot', !!gear && !!chart && !!github && !!theme, feet ? feet.innerHTML.slice(0, 200) : 'no foot');
    if (!gear || !chart || !github || !theme) return done();
    var named = [[gear, 'Settings'], [chart, 'Analytics'], [github, 'Codealong on GitHub']];
    check('icons alone, each named', named.every(function (n) { return n[0].textContent.trim() === '' && n[0].getAttribute('aria-label') === n[1]; }), named.map(function (n) { return n[0].getAttribute('aria-label'); }).join(', '));
    check('in that order, the theme switch after them', gear.getBoundingClientRect().right <= chart.getBoundingClientRect().left && chart.getBoundingClientRect().right <= github.getBoundingClientRect().left && github.getBoundingClientRect().right < theme.getBoundingClientRect().left, '');
    check('at the very foot of the sidebar', Math.abs(feet.getBoundingClientRect().bottom - side.getBoundingClientRect().bottom) <= 1, Math.round(feet.getBoundingClientRect().bottom) + ' of ' + Math.round(side.getBoundingClientRect().bottom));
    github.click();
    await until(function () { return calls('open_url').length === 1; }, 3000, 'the repo opened');
    check('a click on the GitHub link opens the repo', last('open_url').url === REPO, JSON.stringify(sent('open_url')));
    chart.click();
    var usage = await until(function () { return q('.ca-usage'); }, 5000, 'the Analytics window');
    check('a click on Analytics opens its window', !!usage);
    key(document.activeElement || document.body, { key: 'Escape', code: 'Escape' });
    await until(function () { return !q('.ca-usage'); }, 3000, 'the Analytics window shut');

    q('.ca-sidetoggle').click();
    var rail = await until(function () { return q('.ca-rail'); }, 3000, 'the rail');
    var railed = ['Analytics', 'Codealong on GitHub', 'Settings'].map(function (l) { return rail.querySelector('.ca-rail-btn[aria-label^="' + l + '"]'); });
    check('the rail has Analytics, the GitHub link and Settings at its foot, in that order', railed.every(Boolean) && railed[0].nextElementSibling === railed[1] && railed[1].nextElementSibling === railed[2] && rail.lastElementChild === railed[2], railed.map(function (b) { return b ? b.getAttribute('aria-label') : 'missing'; }).join(', '));
    check('nor a line beside it', style(rail).borderRightWidth === '0px' && clear(rail), style(rail).borderRightWidth + ' ' + style(rail).backgroundColor);
    q('.ca-sidetoggle').click();
    await until(function () { return q('.ca-side'); }, 3000, 'the sidebar open again');
    done();
  };
  run().catch(stopped);
})();
