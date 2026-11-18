// Checks the main window's chrome (App.css): the title bar, the sidebar and the status bar are one
// ground, the window's, with no line between them; the board and the panel beside it are a card on
// it, rounded at its four corners, with a gutter at the window's right edge. And the window's own
// controls at the status bar's far end, after Accounts: the theme switch, the GitHub link and
// Settings, icons alone, the last two named for the keyboard and screen readers. A click on the
// GitHub link opens Codealong's repo, and the rail the sidebar folds to has neither. While the
// board is read there's no panel, and the board alone rounds all four corners. At the window's
// least width (980) the grid and its gutter fit, and the status bar's end keeps Settings in the
// window beside notices, a meeting and the machine's load.
//   scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_GLASS=1 scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_LOADING=1 scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_SIZE=980,700 PREVIEW_NOTICES=scripts/preview/notices-sample.json PREVIEW_INIT=scripts/preview/window-chrome-crowded-init.js scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
(function () {
  var REPO = 'https://github.com/codealong/codealong';
  var style = function (el) { return getComputedStyle(el); };
  var clear = function (el) { var b = style(el).backgroundColor; return b === 'rgba(0, 0, 0, 0)' || b === 'transparent'; };
  var rounded = function (el, corners) { return corners.every(function (c) { return parseFloat(style(el)['border' + c + 'Radius']) > 0; }); };
  // The board as the card alone, while the board is being read.
  var alone = async function () {
    var board = await until(function () { return q('.ca-win-body.is-loading > .ca-board'); }, 20000, 'the board, reading');
    check('nothing comes after the board while it reads', board.parentElement.lastElementChild === board, board.parentElement.lastElementChild.className);
    check('the board alone rounds all four corners', rounded(board, ['TopLeft', 'TopRight', 'BottomLeft', 'BottomRight']), ['TopLeft', 'TopRight', 'BottomRight', 'BottomLeft'].map(function (c) { return style(board)['border' + c + 'Radius']; }).join(' '));
    check('and draws its hairline on the right too', style(board).borderRightWidth === '1px', style(board).borderRightWidth);
    check('its clip-path rounds them', /round/.test(style(board).clipPath), style(board).clipPath);
    var edge = window.innerWidth - board.getBoundingClientRect().right;
    check('with the gutter at the window’s right edge', edge >= 4 && edge <= 12, Math.round(edge) + 'px');
    done();
  };
  // The window at its least width: everything in it.
  var narrow = function () {
    var body = q('.ca-win-body');
    var right = Math.max.apply(null, Array.prototype.map.call(body.children, function (c) { return c.getBoundingClientRect().right; }));
    check('at its least width the board, the panel and the gutter fit the window', body.scrollWidth <= body.clientWidth && right <= window.innerWidth - 4, 'children reach ' + Math.round(right) + ' of ' + window.innerWidth + ', scroll ' + body.scrollWidth + ' of ' + body.clientWidth);
    var gear = q('.ca-usagebar-settings').getBoundingClientRect();
    // Crowded (window-chrome-crowded-init.js): the notices and a meeting with a long name.
    var meeting = q('.ca-usagebar-end .ca-meeting.is-line');
    check('the bar is crowded: notices and a meeting beside Settings', !!q('.ca-usagebar-end .ca-notice') && !!meeting, 'notice ' + !!q('.ca-usagebar-end .ca-notice') + ', meeting ' + !!meeting);
    check('and Settings stays in the window', gear.right <= window.innerWidth && gear.left >= q('.ca-usagebar').getBoundingClientRect().left, Math.round(gear.left) + '–' + Math.round(gear.right) + ' of ' + window.innerWidth);
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

    if (window.innerWidth <= 1000) narrow();
    var end = q('.ca-usagebar-end');
    var github = end.querySelector('.ca-usagebar-github');
    var gear = end.querySelector('.ca-usagebar-settings');
    var theme = end.querySelector('.ca-theme');
    check('Settings, the GitHub link and the theme are at the status bar’s end', !!github && !!gear && !!theme, end.innerHTML.slice(0, 200));
    check('Settings is the last of them', end.lastElementChild === gear, end.lastElementChild && end.lastElementChild.className);
    var accounts = end.querySelector('button[aria-label="Accounts"]');
    check('after Analytics and Accounts', !!accounts && !!end.querySelector('button[aria-label="Analytics"]') && theme.getBoundingClientRect().left > accounts.getBoundingClientRect().right, accounts ? 'ok' : 'no Accounts');
    check('Settings and the GitHub link are icons, named', gear.textContent.trim() === '' && gear.getAttribute('aria-label') === 'Settings' && github.textContent.trim() === '' && github.getAttribute('aria-label') === 'Codealong on GitHub', gear.textContent + '|' + github.textContent);
    var icon = function (el) { var svg = el && el.querySelector('svg'); return svg ? svg.innerHTML : ''; };
    check('Accounts no longer wears Settings’ gear', !!accounts && icon(accounts) !== '' && icon(accounts) !== icon(gear), accounts ? icon(accounts).slice(0, 80) : 'no Accounts');
    check('nothing at the sidebar’s foot', !q('.ca-side-foot') && !side.querySelector('[aria-label="Settings"], [aria-label="Codealong on GitHub"], .ca-theme'), 'still there');
    github.click();
    await until(function () { return calls('open_url').length === 1; }, 3000, 'the repo opened');
    check('a click on the GitHub link opens the repo', last('open_url').url === REPO, JSON.stringify(sent('open_url')));

    q('.ca-sidetoggle').click();
    var rail = await until(function () { return q('.ca-rail'); }, 3000, 'the rail');
    check('the rail has neither, the status bar still both', !rail.querySelector('[aria-label="Codealong on GitHub"], [aria-label^="Settings"]') && !!q('.ca-usagebar-end .ca-usagebar-settings'), rail.innerHTML.slice(0, 120));
    check('nor a line beside it', style(rail).borderRightWidth === '0px' && clear(rail), style(rail).borderRightWidth + ' ' + style(rail).backgroundColor);
    q('.ca-sidetoggle').click();
    await until(function () { return q('.ca-side'); }, 3000, 'the sidebar open again');
    done();
  };
  run().catch(stopped);
})();
