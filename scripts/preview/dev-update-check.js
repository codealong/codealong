// Checks Codealong Dev's Update (ui/Updates.tsx `SidebarUpdate`): once the engine says origin/main
// has commits the build hasn't (updates.rs `dev_check`, pushed here as it would), the sidebar
// offers it, and a click opens the quick panel on the source's checkout with agentTasks.ts's
// prompt filled in (api.ts `askAgent`), as the other buttons that ask an agent do. A development
// build that has them all offers nothing.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/dev-update-check.js
(function () {
  var status = function (s) { window.__emit('update-status', (window.__update = Object.assign({ current: '0.6.0' }, s))); };
  var update = function () { return q('.ca-side-update'); };
  var behind = { built: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b', commits: 3, source: '/Users/me/projects/codealong', checkedAt: Date.now() };

  (async function () {
    try {
      await until(function () { return q('.ca-side'); }, 20000, 'the sidebar');
      status({ state: 'off' });
      await wait(300);
      check('a development build with everything origin/main has offers nothing', !update(), update() ? update().textContent : '');
      status({ state: 'off', behind: behind });
      await until(update, 3000, 'no Update in the sidebar');
      check('one origin/main has moved past offers Update', update().textContent.trim() === 'Update: 3 new commits', update().textContent);
      check('naming the commit it was built from', /\(1a2b3c4d\)/.test(update().title), update().title);
      update().click();
      await until(function () { return calls('show_quick').length > 0; }, 3000, 'Update opened no quick panel');
      var open = calls('show_quick')[0][1].context;
      check('on the source’s checkout', open.laneId === '/Users/me/projects/codealong' && !open.chat, JSON.stringify(open));
      check('with the prompt filled in', /^Update Codealong Dev to the latest origin\/main and relaunch it\. It was built from 1a2b3c4d, and origin\/main has 3 commits since\./.test(open.prompt || ''), (open.prompt || '').slice(0, 160));
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
