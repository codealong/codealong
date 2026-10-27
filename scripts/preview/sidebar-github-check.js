// Checks the GitHub button at the foot of the sidebar (App.tsx's foot, Sidebar.tsx's rail): it
// sits beside Settings, the theme switch still at the right, and a click opens Codealong's repo
// in the browser, from the open sidebar and from the rail it folds to.
//   scripts/preview.sh sessions '' scripts/preview/sidebar-github-check.js
(function () {
  var REPO = 'https://github.com/codealong/codealong';
  var run = async function () {
    // The window lays itself out again once the board is read, so it's looked at after that.
    await until(function () { return q('.ca-sessionlist .ca-lane') && q('.ca-sidetoggle'); }, 20000, 'the sessions');
    await wait(300);
    if (!q('.ca-side-foot')) {
      q('.ca-sidetoggle').click();
    }
    var button = await until(function () { return q('.ca-side-foot .ca-side-github'); }, 3000, 'the GitHub button at the sidebar’s foot');
    var settings = q('.ca-side-foot .ca-side-config').getBoundingClientRect();
    var github = button.getBoundingClientRect();
    var theme = q('.ca-side-foot .ca-theme').getBoundingClientRect();
    check('it sits just after Settings', github.left > settings.right && github.left - settings.right < 24, Math.round(github.left - settings.right) + 'px after');
    check('the theme switch stays at the right', theme.left > github.right + 24, Math.round(theme.left - github.right) + 'px between');
    check('it says where it goes', button.getAttribute('title') === 'Codealong on GitHub', button.getAttribute('title'));
    button.click();
    await until(function () { return calls('open_url').length === 1; }, 3000, 'the repo opened');
    check('a click opens the repo', last('open_url').url === REPO, JSON.stringify(sent('open_url')));

    q('.ca-sidetoggle').click();
    var rail = await until(function () { return q('.ca-rail-btn[aria-label="Codealong on GitHub"]'); }, 3000, 'the GitHub button on the folded rail');
    var cog = q('.ca-rail-btn[aria-label^="Settings"]');
    check('on the rail it sits above Settings', rail.nextElementSibling === cog, cog ? cog.getAttribute('aria-label') : 'no Settings');
    rail.click();
    await until(function () { return calls('open_url').length === 2; }, 3000, 'the repo opened from the rail');
    check('a click on the rail opens the repo', last('open_url').url === REPO, JSON.stringify(sent('open_url')));
    q('.ca-sidetoggle').click();
    await until(function () { return q('.ca-side-foot'); }, 3000, 'the sidebar open again');
    done();
  };
  run().catch(stopped);
})();
