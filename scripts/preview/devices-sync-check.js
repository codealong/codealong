// Checks Sync projects on a linked machine's card in Settings › Devices (DevicesSection.tsx): it
// opens the quick panel on a chat with agentTasks.ts's prompt filled in (api.ts `askAgent`), the
// machine's projects listed in it as it names them (peers.rs `projects_there`, devices-sample.json
// has the Mac mini's), so you pick the agent that matches this machine's to them. A machine that
// hasn't sent its worktrees has nothing to match.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/devices-sync-check.js
(function () {
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var card = function (name) {
    return all('.ca-settings .ca-ucard').filter(function (c) { var n = c.querySelector('.ca-device-name'); return n && n.textContent === name; })[0];
  };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };

  (async function () {
    try {
      await until(function () { return q('.ca-side-config'); }, 20000, 'no Settings button');
      q('.ca-side-config').click();
      await until(function () { return nav('Devices'); }, 3000, 'no Devices in Settings');
      nav('Devices').click();
      await until(function () { return card('Mac mini') && button('Sync projects…', card('Mac mini')); }, 5000, 'no Sync projects on Mac mini’s card');

      var sync = button('Sync projects…', card('Mac mini'));
      check('a machine whose worktrees came offers it', !sync.disabled, sync.title);
      sync.click();
      await until(function () { return calls('show_quick').length > 0; }, 3000, 'Sync projects opened no quick panel');
      var open = calls('show_quick')[0][1].context;
      check('in a chat of its own', open.chat === true && !open.laneId, JSON.stringify(open));
      check('asking to match Mac mini’s projects', /^Make this machine’s Codealong projects match my Mac mini’s/.test(open.prompt || ''), (open.prompt || '').slice(0, 120));
      var listed = (open.prompt || '').split('\n').filter(function (l) { return /^- [^:]+: /.test(l); });
      check('with its projects listed as it names them', listed.join(' | ') === '- Northwind: github.com/northwind/app, github.com/northwind/api | - Atlas: github.com/atlas/web, /Users/me/projects/atlas-notes', listed.join(' | '));
      var studio = button('Sync projects…', card('Studio'));
      check('a machine with no projects sent offers nothing to match', !!studio && studio.disabled, studio ? studio.title : 'no button');
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
