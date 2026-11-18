// Checks the main window's chrome (App.css): the title bar, the sidebar and the status bar are one
// ground, the window's, with no line between them; the board and the panel beside it are a card on
// it, rounded at its four corners, with a gutter at the window's right edge. And the window's own
// controls at the status bar's far end, after Accounts: the theme switch, the GitHub link and
// Settings, icons alone, the last two named for the keyboard and screen readers. A click on the
// GitHub link opens Codealong's repo, and the rail the sidebar folds to has neither.
//   scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
//   PREVIEW_GLASS=1 scripts/preview.sh sessions '' scripts/preview/window-chrome-check.js
(function () {
  var REPO = 'https://github.com/codealong/codealong';
  var style = function (el) { return getComputedStyle(el); };
  var clear = function (el) { var b = style(el).backgroundColor; return b === 'rgba(0, 0, 0, 0)' || b === 'transparent'; };
  var run = async function () {
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

    var end = q('.ca-usagebar-end');
    var github = end.querySelector('.ca-usagebar-github');
    var gear = end.querySelector('.ca-usagebar-settings');
    var theme = end.querySelector('.ca-theme');
    check('Settings, the GitHub link and the theme are at the status bar’s end', !!github && !!gear && !!theme, end.innerHTML.slice(0, 200));
    check('Settings is the last of them', end.lastElementChild === gear, end.lastElementChild && end.lastElementChild.className);
    var accounts = Array.from(end.querySelectorAll('button')).find(function (x) { return x.textContent.trim() === 'Accounts'; });
    check('after Analytics and Accounts', !!accounts && theme.getBoundingClientRect().left > accounts.getBoundingClientRect().right, accounts ? 'ok' : 'no Accounts');
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
