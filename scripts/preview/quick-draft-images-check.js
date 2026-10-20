// Checks that the quick panel's draft keeps its images across a relaunch (Quick.tsx; quick.ts
// `nextDraft`, `draftCodec`): images pasted in go into the kept text, and Improve Codealong's
// pictures of the windows, which aren't part of the text, are kept beside it. ↑ brings a draft
// back with its pictures and Improve's start in Codealong's source, and walking on puts back the
// panel's own pictures (one taken off stays off) and where it was starting.
//
//   scripts/preview.sh quick '' scripts/preview/quick-draft-images-check.js
// The board needs a worktree in a project.
(function () {
  // This runs before the app's script, so the page comes up as a relaunched one does, with a
  // draft kept from before: one written over Improve Codealong's pictures.
  var KEY = 'codealong.quick.draft';
  var KEPT = 'The bar is too tall';
  var OLDER = 'An older prompt';
  var SHOTS = [
    { kind: 'image', path: 'images/aaaaaaaaaaaaaaaa.png', name: 'Main window' },
    { kind: 'image', path: 'images/bbbbbbbbbbbbbbbb.png', name: 'Floating bar' },
  ];
  localStorage.setItem(KEY, JSON.stringify({ text: KEPT, todos: [], shots: SHOTS }));
  localStorage.setItem('codealong.quick.history', JSON.stringify([OLDER]));
  localStorage.setItem('codealong.quick.project', 'All projects');

  var input = function () { return q('.ca-quick-input textarea'); };
  var thumbs = function () { return all('.ca-quick-attached .ca-imgthumb').length; };
  var projectChip = function () { return q('.ca-quick-project span').textContent; };
  var stored = function () {
    var raw = localStorage.getItem(KEY);
    return raw ? JSON.parse(raw) : null;
  };
  var up = function () { key(input(), { key: 'ArrowUp', code: 'ArrowUp' }); return wait(150); };
  var down = function () { key(input(), { key: 'ArrowDown', code: 'ArrowDown' }); return wait(150); };
  var improving = function () { return input().placeholder === 'Describe how to improve Codealong…'; };
  var shown = function () { return JSON.stringify(input().value) + ', ' + thumbs() + ' pictures, on ' + projectChip(); };

  (async function () {
    try {
      say('quick draft images check: waiting for the panel');
      await until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel');
      var board = await window.__TAURI_INTERNALS__.invoke('get_board');
      var lane = board.lanes.filter(function (l) { return !l.hidden && !l.startsChats && !l.device && l.project; })[0];
      if (!lane) throw new Error('no worktree in a project on the board');
      // Codealong's source in that worktree, as the board says where Improve starts.
      var withSource = Object.assign({}, board, { improve: { project: lane.project, laneId: lane.id } });
      window.__quickOpen({}, withSource);
      await wait(200);
      check('the panel opens empty, without pictures, on all projects', input().value === '' && thumbs() === 0 && projectChip() === 'All projects', shown());

      await up();
      check('↑ brings back the draft kept before the relaunch', input().value === KEPT, shown());
      check('with the pictures it was written over', thumbs() === 2, shown());
      check('as Improve Codealong, on its source’s project', improving() && projectChip() === lane.project, input().placeholder + ', ' + shown());
      await up();
      check('the prompt before it comes without them, where the panel was', input().value === OLDER && thumbs() === 0 && projectChip() === 'All projects', shown());
      await down();
      check('↓ brings the draft back with its pictures and project', input().value === KEPT && thumbs() === 2 && projectChip() === lane.project, shown());
      await down();
      check('and the empty input has neither', input().value === '' && thumbs() === 0 && projectChip() === 'All projects', shown());

      // With the source on no worktree the board has, the draft starts nowhere, as an Improve
      // open does: never in the worktree or the chats the panel was opened on.
      var noSource = Object.assign({}, board, { improve: {} });
      var bar = function () { return q('.ca-quick-bar').textContent; };
      var startable = function () { return !!q('.ca-quick-compose') && !q('.ca-quick-compose').classList.contains('is-disabled'); };
      window.__quickOpen({ laneId: lane.id, project: lane.project }, noSource);
      await wait(200);
      var opened = bar();
      await up();
      check('without the source, the draft has no worktree to start in', input().value === KEPT && bar().indexOf('No worktree') >= 0 && !startable(), bar().slice(0, 120));
      await down();
      check('and walking off it is back on the worktree it was opened on', bar() === opened && bar().indexOf('No worktree') < 0, bar().slice(0, 120));
      window.__quickOpen({ chat: true }, noSource);
      await wait(200);
      await up();
      check('nor in the chats the panel was opened for', input().value === KEPT && bar().indexOf('No worktree') >= 0 && !startable(), bar().slice(0, 120));
      await down();

      // Improve Codealong opened afresh, with pictures of its own, one taken off.
      window.__quickOpen({ improve: { shots: SHOTS } }, withSource);
      await until(function () { return thumbs() === 2; }, 5000, 'Improve’s pictures');
      q('.ca-imgthumb-x', all('.ca-quick-attached .ca-imgthumb')[1]).click();
      await until(function () { return thumbs() === 1; }, 3000, 'taking a picture off');
      await up();
      check('↑ brings the draft back with its own pictures', input().value === KEPT && thumbs() === 2, shown());
      await up();
      check('walking on puts back the open’s pictures, without the one taken off', input().value === OLDER && thumbs() === 1, shown());
      type(input(), 'Make the notch wider');
      await wait(150);
      var s = stored();
      check('what is typed over the pictures is kept with them', s && s.text === 'Make the notch wider' && JSON.stringify(s.shots) === JSON.stringify([SHOTS[0]]), JSON.stringify(s));

      window.__quickOpen({}, withSource);
      await wait(150);
      check('the next open starts without them', input().value === '' && thumbs() === 0, shown());
      await up();
      check('↑ brings them back with the words', input().value === 'Make the notch wider' && thumbs() === 1, shown());
      await down();

      // An image pasted into a plain draft goes into the kept text.
      var dt = new DataTransfer();
      dt.items.add(new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'shot.png', { type: 'image/png' }));
      input().dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
      await until(function () { return thumbs() === 1; }, 5000, 'the pasted image');
      type(input(), 'Look at this');
      await wait(150);
      s = stored();
      check('a pasted image is kept in the draft', s && /!\[image\]\(images\/0123456789abcdef\.png\)/.test(s.text) && !s.shots, JSON.stringify(s));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
