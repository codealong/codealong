// Checks the quick panel opened to improve Codealong (Quick.tsx, improve.ts): the input asks how
// to improve Codealong, the panel is on the board's Codealong project and worktree, the pictures
// the engine took of the windows show as attachments and go into the prompt after what you
// typed, one removed stays out; with Codealong's source in no project the panel says so and its
// button saves config.toml with a Codealong project for the source; with no source it says where
// to clone one; with it only on another Mac the panel goes there and starts there with the
// pictures; and an ordinary open after is itself again.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/improve-check.js
// The board needs a worktree in a project and an account that can start.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var projectChip = function () { return q('.ca-quick-project span'); };
  var thumbs = function () { return all('.ca-quick-attached .ca-imgthumb'); };
  var box = function () { return q('.ca-quick-improve'); };
  var compose = function () { return q('.ca-quick-compose'); };
  var SHOTS = [
    { kind: 'image', name: 'Main window', path: 'images/0000000000000000000000000000000000000000000000000000000000000001.png' },
    { kind: 'image', name: 'Floating bar', path: 'images/0000000000000000000000000000000000000000000000000000000000000002.png' },
  ];
  var board, lane;

  (async function () {
    try {
      await until(function () { return window.__quickOpen && q('.ca-quick-input textarea'); }, 20000, 'the quick panel’s page');
      board = await window.__TAURI_INTERNALS__.invoke('get_board');
      lane = board.lanes.filter(function (l) { return !l.hidden && !l.startsChats && !l.device && l.project; })[0];
      if (!lane) throw new Error('no worktree in a project on the board');

      // On Codealong's project and worktree, with the pictures.
      var onProject = Object.assign({}, board, { improve: { project: lane.project, laneId: lane.id } });
      window.__quickOpen({ improve: { shots: SHOTS } }, onProject);
      await until(function () { return thumbs().length === 2; }, 5000, 'the pictures as attachments');
      check('the input asks how to improve Codealong', input().placeholder === 'Describe how to improve Codealong…', input().placeholder);
      check('on the project Codealong’s source is in', projectChip().textContent === lane.project, projectChip().textContent);
      check('the pictures show as attachments', thumbs().length === 2);
      check('no note while the source is in a project', !box());
      // One picture taken off stays out of the prompt.
      q('.ca-imgthumb-x', thumbs()[1]).click();
      await until(function () { return thumbs().length === 1; }, 3000, 'removing a picture');
      type(input(), 'Make the bar taller.');
      await until(function () { return compose() && !compose().classList.contains('is-disabled'); }, 10000, 'a start that’s ready (an account and a place)');
      var starts = calls('start_from_quick').length;
      key(input(), { key: 'Enter', metaKey: true });
      await until(function () { return calls('start_from_quick').length > starts; }, 5000, '⌘⏎ starting');
      var spec = calls('start_from_quick').pop()[1].spec;
      check('the start is in that worktree', spec.laneId === lane.id, spec.laneId);
      check('the prompt is what you typed, then the pictures named for their windows', spec.prompt === 'Make the bar taller.\n\nCodealong’s windows as they were when this was asked:\n![Main window](' + SHOTS[0].path + ')', JSON.stringify(spec.prompt));

      // The source found, in no project: the note offers a project and saves it.
      var noProject = Object.assign({}, board, { improve: { source: { root: '/me/projects/codealong', repo: 'github.com/me/codealong' } } });
      window.__quickOpen({ improve: { shots: SHOTS } }, noProject);
      await until(box, 5000, 'the note about the source in no project');
      check('the note names the source’s folder', box().textContent.indexOf('/me/projects/codealong') >= 0, box().textContent);
      var add = q('button', box());
      check('with a button that makes a project', !!add && add.textContent.trim() === 'Add a Codealong project', add && add.textContent);
      var saves = calls('save_config').length;
      add.click();
      await until(function () { return calls('save_config').length > saves; }, 5000, 'Add a Codealong project saving config.toml');
      var projects = calls('save_config').pop()[1].config.projects;
      var made = projects.filter(function (p) { return p.name === 'Codealong'; })[0];
      check('config.toml gets a Codealong project with the source’s repo', !!made && made.repos.indexOf('github.com/me/codealong') >= 0, JSON.stringify(projects.map(function (p) { return p.name; })));

      // No source at all: where to clone one, and no worktree to start in by default (never
      // the project's first, which is some other repo).
      window.__quickOpen({ improve: { shots: [] } }, Object.assign({}, board, { improve: {} }));
      await until(function () { return box() && !q('button', box()); }, 5000, 'the note about cloning the source');
      check('without the source it says where to clone it', box().textContent.indexOf('github.com/codealong/codealong') >= 0, box().textContent);
      check('and has no button', !q('button', box()));
      check('and no worktree to start in', compose().classList.contains('is-disabled') && q('.ca-quick-bar').textContent.indexOf('No worktree') >= 0, compose().textContent.slice(0, 80));

      // The source on another Mac and none here: the panel goes there and starts there, the
      // pictures going with the prompt (actions.rs `carry_images`).
      var devices = await window.__TAURI_INTERNALS__.invoke('devices');
      var mini = { key: 'ab'.repeat(32), short: 'abcdef0123', name: 'Mini', online: true, via: 'tailscale', since: Date.now(), lastSeen: null, remoteControl: true, app: '0.1.0', linkedBack: true, loading: false, usage: [], color: 'teal', kind: 'mac' };
      var there = Object.assign({}, lane, { id: '@' + mini.short + ':/Users/sam/codealong', codealong: true, origin: 'main', hidden: false, startsChats: false, sessions: [], device: { name: mini.name, short: mini.short, color: mini.color, online: true, lastSeen: null } });
      // The panel reads your devices as it opens, so they're answered from here meanwhile.
      var withMini = Object.assign({}, devices, { enabled: true, devices: [mini], lanes: [there] });
      var invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd) { return cmd === 'devices' ? Promise.resolve(withMini) : invoke.apply(this, arguments); };
      window.__emit('devices', withMini);
      window.__quickOpen({ improve: { shots: SHOTS } }, Object.assign({}, board, { improve: {} }));
      await until(function () { return box() && box().textContent.indexOf('Mini') >= 0; }, 5000, 'the note about the source on Mini');
      check('the note says the session starts on the Mac with the source', box().textContent.indexOf('starts there and the pictures go with it') >= 0, box().textContent);
      type(input(), 'Make the bar taller.');
      await until(function () { return compose() && !compose().classList.contains('is-disabled'); }, 10000, 'a start on Mini that’s ready');
      starts = calls('start_from_quick').length;
      key(input(), { key: 'Enter', metaKey: true });
      await until(function () { return calls('start_from_quick').length > starts; }, 5000, '⌘⏎ starting on Mini');
      spec = calls('start_from_quick').pop()[1].spec;
      check('the start is in Mini’s worktree, with the pictures', spec.laneId === there.id && spec.prompt.indexOf(SHOTS[0].path) >= 0, spec.laneId + ' ' + JSON.stringify(spec.prompt));
      window.__TAURI_INTERNALS__.invoke = invoke;
      window.__emit('devices', devices);

      // An ordinary open is itself again.
      window.__quickOpen({}, board);
      await wait(200);
      check('an ordinary open asks for a session or new work', input().placeholder.indexOf('Find a session') === 0, input().placeholder);
      check('with no pictures and no note', thumbs().length === 0 && !box());
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
