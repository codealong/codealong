// Checks Switch to Codealong Dev? (DevSetupDialog.tsx, Quick.tsx): Improve Codealong in the
// installed app (the board's `improve.installed`) asks first, inside the panel's card; Set it up
// starts a session on the panel's account with improve.ts's `devSetupPrompt`, in the source's
// checkout when this machine has one and in a chat of its own to clone one when it doesn't, never
// on another Mac that has the source; Not now and Escape leave the panel as Improve opens it;
// Don't suggest this again keeps it from asking; and a build of the source never asks.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/dev-setup-check.js
// The board needs a worktree in a project and an account that can start.
(function () {
  var NO_DEV_KEY = 'codealong.quick.no-dev-setup';
  localStorage.removeItem(NO_DEV_KEY);
  var dialog = function () { return q('.ca-quick > .ca-scrim .ca-devsetup'); };
  var input = function () { return q('.ca-quick-input textarea'); };
  var primary = function () { return q('.ca-btn--primary', dialog()) || q('button[type="submit"]', dialog()); };
  var buttonNamed = function (text) { return all('button', dialog()).filter(function (b) { return b.textContent.trim() === text; })[0]; };

  (async function () {
    try {
      await until(function () { return window.__quickOpen && input(); }, 20000, 'the quick panel’s page');
      var board = await window.__TAURI_INTERNALS__.invoke('get_board');
      var config = await window.__TAURI_INTERNALS__.invoke('get_config');
      var lane = board.lanes.filter(function (l) { return !l.hidden && !l.startsChats && !l.device && l.project; })[0];
      if (!lane) throw new Error('no worktree in a project on the board');
      // A chat of its own starts in the chats lane, which the board has with no sessions too.
      var chats = board.lanes.filter(function (l) { return l.startsChats && !l.device; })[0];
      if (!chats) {
        chats = Object.assign({}, lane, { id: '/me/Library/Application Support/Codealong/chats', path: '/me/Library/Application Support/Codealong/chats', project: '', startsChats: true, chats: true, hidden: true, sessions: [] });
        board = Object.assign({}, board, { lanes: board.lanes.concat([chats]) });
      }
      var installed = function (improve) { return Object.assign({}, board, { improve: Object.assign({ installed: true }, improve) }); };

      // A build of the source never asks.
      window.__quickOpen({ improve: { shots: [] } }, Object.assign({}, board, { improve: { project: lane.project, laneId: lane.id } }));
      await wait(300);
      check('a build of the source doesn’t ask', !dialog());

      // The installed app, with the source in one of your projects: it asks, inside the card.
      window.__quickOpen({ improve: { shots: [] } }, installed({ project: lane.project, laneId: lane.id }));
      await until(dialog, 5000, 'Switch to Codealong Dev?');
      check('it asks whether to switch to Codealong Dev', q('h2', dialog()).textContent === 'Switch to Codealong Dev?', q('h2', dialog()).textContent);
      var scrim = q('.ca-quick > .ca-scrim');
      var card = q('.ca-quick').getBoundingClientRect();
      var cover = scrim.getBoundingClientRect();
      check('it covers the card, not the window round it', Math.abs(cover.left - card.left) < 1 && Math.abs(cover.width - card.width) < 1 && Math.abs(cover.height - card.height) < 1, JSON.stringify([cover.left, cover.width, cover.height, card.left, card.width, card.height]));
      check('it says the session works in the source’s checkout', dialog().textContent.indexOf(lane.path.replace(/^\/Users\/[^/]+/, '~')) >= 0, q('.ca-devsetup-note').textContent);
      await until(function () { return primary() && !primary().disabled; }, 10000, 'Set it up with an account');
      check('the button names the account it starts on', /^Set it up with \S+/.test(primary().textContent.trim()), primary().textContent);
      // The panel's own keys wait: ⌘P doesn't open the project menu behind it.
      key(document.activeElement || document.body, { key: 'p', metaKey: true });
      await wait(100);
      check('the panel’s keys wait while it asks', !q('.ca-picker'));
      var starts = calls('start_from_quick').length;
      primary().click();
      await until(function () { return calls('start_from_quick').length > starts; }, 5000, 'Set it up starting a session');
      var spec = calls('start_from_quick').pop()[1].spec;
      check('it starts in the source’s checkout', spec.laneId === lane.id && !spec.worktree, spec.laneId);
      check('on the prompt that builds Codealong Dev from it', spec.prompt.indexOf('Codealong’s source is already in ' + lane.path) >= 0 && spec.prompt.indexOf('scripts/relaunch.sh') >= 0, JSON.stringify(spec.prompt.slice(0, 300)));
      check('with nothing of the panel’s added', spec.prompt.indexOf('images/') < 0);

      // No source on this machine, but on another Mac: it starts in a chat of its own here, to
      // clone the source into the folder Codealong looks for repos in.
      var devices = await window.__TAURI_INTERNALS__.invoke('devices');
      var mini = { key: 'ab'.repeat(32), short: 'abcdef0123', name: 'Mini', online: true, via: 'tailscale', since: Date.now(), lastSeen: null, remoteControl: true, app: '0.1.0', linkedBack: true, loading: false, usage: [], color: 'teal', kind: 'mac' };
      var there = Object.assign({}, lane, { id: '@' + mini.short + ':/Users/sam/codealong', codealong: true, origin: 'main', hidden: false, startsChats: false, sessions: [], device: { name: mini.name, short: mini.short, color: mini.color, online: true, lastSeen: null } });
      var withMini = Object.assign({}, devices, { enabled: true, devices: [mini], lanes: [there] });
      var invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd) { return cmd === 'devices' ? Promise.resolve(withMini) : invoke.apply(this, arguments); };
      window.__emit('devices', withMini);
      window.__quickOpen({ improve: { shots: [] } }, installed({}));
      await until(dialog, 5000, 'Switch to Codealong Dev? with no source here');
      check('without the source it says the agent gets it from GitHub', q('.ca-devsetup-note').textContent.indexOf('gets the source from GitHub') >= 0, q('.ca-devsetup-note').textContent);
      await wait(300);
      check('the panel stays on this machine while it asks', q('.ca-quick-bar').textContent.indexOf('Mini') < 0, q('.ca-quick-bar').textContent.slice(0, 120));
      await until(function () { return primary() && !primary().disabled; }, 10000, 'Set it up with an account');
      starts = calls('start_from_quick').length;
      primary().click();
      await until(function () { return calls('start_from_quick').length > starts; }, 5000, 'Set it up starting a chat');
      spec = calls('start_from_quick').pop()[1].spec;
      check('it starts in a chat of its own on this machine', spec.laneId === chats.id, spec.laneId);
      var into = config.config.scan.length === 1 ? 'into ' + config.config.scan[0] + ',' : config.config.scan.length > 1 ? '(' + config.config.scan.join(', ') + ')' : 'ask me if you can’t tell';
      check('on the prompt that clones the source where Codealong looks for repos', spec.prompt.indexOf('Clone https://github.com/codealong/codealong') >= 0 && spec.prompt.indexOf(into) >= 0, JSON.stringify(spec.prompt.slice(0, 300)));

      // Not now: Improve Codealong as it opens, the panel then going to the Mac with the source.
      window.__quickOpen({ improve: { shots: [] } }, installed({}));
      await until(dialog, 5000, 'Switch to Codealong Dev? again');
      var hides = calls('hide_quick').length;
      buttonNamed('Not now').click();
      await until(function () { return !dialog(); }, 3000, 'Not now closing it');
      check('Not now leaves the panel up, asking how to improve Codealong', calls('hide_quick').length === hides && input().placeholder === 'Describe how to improve Codealong…', input().placeholder);
      await until(function () { return q('.ca-quick-bar').textContent.indexOf('Mini') >= 0; }, 5000, 'the panel going to Mini after');
      check('and then goes to the Mac with the source, as Improve does', true);
      window.__TAURI_INTERNALS__.invoke = invoke;
      window.__emit('devices', devices);

      // Escape closes it, not the panel.
      window.__quickOpen({ improve: { shots: [] } }, installed({ project: lane.project, laneId: lane.id }));
      await until(dialog, 5000, 'Switch to Codealong Dev? for Escape');
      hides = calls('hide_quick').length;
      key(document.activeElement || document.body, { key: 'Escape' });
      await until(function () { return !dialog(); }, 3000, 'Escape closing it');
      check('Escape closes it and leaves the panel up', calls('hide_quick').length === hides);

      // Don't suggest this again: Not now with it ticked, and it doesn't ask again.
      window.__quickOpen({ improve: { shots: [] } }, installed({ project: lane.project, laneId: lane.id }));
      await until(dialog, 5000, 'Switch to Codealong Dev? for Don’t suggest');
      q('.ca-check input', dialog()).click();
      buttonNamed('Not now').click();
      await until(function () { return !dialog(); }, 3000, 'Not now closing it');
      window.__quickOpen({ improve: { shots: [] } }, installed({ project: lane.project, laneId: lane.id }));
      await wait(300);
      check('once told not to, it doesn’t ask again', !dialog() && localStorage.getItem(NO_DEV_KEY) === '1');
      localStorage.removeItem(NO_DEV_KEY);

      // An ordinary open never asks.
      window.__quickOpen({}, installed({ project: lane.project, laneId: lane.id }));
      await wait(300);
      check('an ordinary open in the installed app doesn’t ask', !dialog());
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
