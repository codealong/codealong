// Checks a folder that isn't on the board, picked in the quick panel's worktree menu (Quick.tsx,
// quick.ts `folderChoices`, newfolder.rs): Another folder… comes after No project and puts the
// folder most main checkouts are in into the filter; a path typed there lists the folders in it
// whose names start with what’s typed, ahead of making a folder of a name half typed, a click on
// one going on from it; a new folder is offered as a project of its own and in the project the
// panel is on, an existing one to add, one whose parent isn't there is refused, and a worktree's
// own folder is that worktree. ⏎ adds the first (add_folder, a project of its own), and until the
// board has it the chip says it's being added and the start row starts nothing; a worktree picked
// before an add's answer comes stays picked, and starts there. The disk is fake-tauri.js's:
// ~/projects with atlas and atlas-web in it.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-new-folder-check.js
// The board needs a project with a worktree in it.
(function () {
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘T\)|Adding/.test(b.title); })[0]; };
  var filter = function () { return q('.ca-picker-filter'); };
  var options = function () { return all('.ca-picker-opt'); };
  var said = function (o) { return o.querySelector('.ca-picker-title').textContent + ' · ' + ((o.querySelector('.ca-picker-detail') || {}).textContent || ''); };
  var rows = function () { return options().map(said); };
  var named = function (title) { return options().filter(function (o) { return o.querySelector('.ca-picker-title').textContent === title; })[0]; };
  var typeIn = function (text, until_) {
    type(filter(), text);
    return until(until_, 5000, 'rows for ' + text);
  };
  var project = null;
  var lane = null;

  // The panel on a project of the board's that has a worktree of this Mac's.
  fetch('./board.json')
    .then(function (r) { return r.json(); })
    .then(function (board) {
      lane = board.lanes.filter(function (l) { return !l.device && !l.hidden && !l.startsChats && l.project !== 'Elsewhere' && l.origin === 'main'; })[0];
      if (!lane) throw new Error('no main checkout on the board to type the folder of');
      project = lane.project;
      localStorage.setItem('codealong.quick.project', project);
      say('quick new folder check: on ' + project + ', waiting for the panel');
      // The panel's open, which comes after it loads, closes any menu that's up.
      return until(function () { return !!input() && !!chip() && !chip().disabled && calls('quick_ready').length > 0; }, 20000, 'the panel’s open');
    })
    .then(function () {
      chip().click();
      return until(function () { return options().length > 0; }, 5000, 'the worktree menu');
    })
    .then(function () {
      var o = named('Another folder…');
      var titles = options().map(function (x) { return x.querySelector('.ca-picker-title').textContent; });
      check('the menu offers another folder', !!o, titles.slice(0, 3).join(' | '));
      check('after No project, ahead of the worktrees', titles.indexOf('Another folder…') === (titles[0] === 'No project' ? 1 : 0), titles.slice(0, 3).join(' | '));
      check('the filter says it takes a path', /folder’s path/.test(filter().placeholder), filter().placeholder);
      o.click();
      return wait(100);
    })
    .then(function () {
      check('picked, it puts a folder in the filter to go on from', /\/$/.test(filter().value) && (filter().value === '~/' || filter().value.indexOf('~/') === 0 || filter().value.indexOf('/') === 0), filter().value);
      check('and the menu stays open', !!filter());
      return typeIn('~/projects/at', function () { return !!named('atlas/'); });
    })
    .then(function () {
      check('a name typed lists the folders that start with it', rows().slice(0, 2).join(' | ') === 'atlas/ · ~/projects/atlas | atlas-web/ · ~/projects/atlas-web', rows().join(' | '));
      check('ahead of making a folder of it', rows()[2] === 'at · New folder ~/projects/at · a project of its own', rows().join(' | '));
      named('atlas/').click();
      return until(function () { return filter().value === '~/projects/atlas/'; }, 3000, 'the folder in the filter');
    })
    .then(function () {
      return until(function () { return !!named('atlas'); }, 5000, 'atlas offered');
    })
    .then(function () {
      check('a folder that’s there is offered to add', rows()[0] === 'atlas · Add ~/projects/atlas · a project of its own', rows().join(' | '));
      check('in the project the panel is on too', rows()[1] === 'atlas · Add ~/projects/atlas · in ' + project, rows().join(' | '));
      return typeIn('~/projects/kestrel', function () { return !!named('kestrel'); });
    })
    .then(function () {
      check('a new one is a new folder', rows().join(' | ') === 'kestrel · New folder ~/projects/kestrel · a project of its own | kestrel · New folder ~/projects/kestrel · in ' + project, rows().join(' | '));
      check('the first is the one ⏎ picks', q('.ca-picker-opt.is-on') === options()[0]);
      return typeIn('~/projets/kestrel', function () { return options().length > 0 && /isn’t there/.test(rows()[0]); });
    })
    .then(function () {
      var o = options()[0];
      check('a typo above it is refused', rows()[0] === '~/projets isn’t there. · ' && o.disabled, rows().join(' | '));
      var mine = lane.path.replace(/^\/Users\/[^/]+/, '~');
      return typeIn(mine, function () { return options().length > 0 && rows()[0].indexOf(lane.callsign) === 0; }).then(function () { return mine; });
    })
    .then(function (mine) {
      check('a worktree’s own folder is that worktree', rows().length === 1 && rows()[0].indexOf(lane.callsign) === 0, mine + ': ' + rows().join(' | '));
      return typeIn('~/projects/kestrel', function () { return !!named('kestrel'); });
    })
    .then(function () {
      key(filter(), { key: 'Enter' });
      return until(function () { return calls('add_folder').length > 0; }, 3000, 'add_folder');
    })
    .then(function () {
      var asked = last('add_folder');
      check('⏎ adds it as a project of its own', asked.typed === '~/projects/kestrel' && asked.project === null, JSON.stringify(asked));
      check('the menu closes', !filter());
      return until(function () { return /Adding kestrel/.test(chip().title); }, 3000, 'the chip saying it’s adding');
    })
    .then(function () {
      check('the chip names it while it’s added', chip().textContent.indexOf('kestrel') >= 0, chip().textContent);
      type(input(), 'Set up a new CLI for kestrel');
      key(input(), { key: 'Enter', metaKey: true });
      return wait(300);
    })
    .then(function () {
      var info = q('.ca-quick-error.is-info');
      check('the start row waits for it', calls('start_from_quick').length === 0, calls('start_from_quick').length + ' starts');
      check('and says why', !!info && /Still adding kestrel/.test(info.textContent), info && info.textContent);
      // Another folder added slowly, and a worktree picked before its answer comes.
      window.__ADD_FOLDER_DELAY__ = 1500;
      chip().click();
      return until(function () { return options().length > 0; }, 5000, 'the worktree menu again');
    })
    .then(function () {
      return typeIn('~/projects/northwind', function () { return !!named('northwind'); });
    })
    .then(function () {
      key(filter(), { key: 'Enter' });
      return until(function () { return calls('add_folder').length > 1 && /Adding northwind/.test(chip().title); }, 3000, 'northwind being added');
    })
    .then(function () {
      chip().click();
      return until(function () { return !!named(lane.callsign + ' (main checkout)'); }, 5000, lane.callsign + ' in the menu');
    })
    .then(function () {
      named(lane.callsign + ' (main checkout)').click();
      return wait(2500);
    })
    .then(function () {
      check('a worktree picked while a folder is added stays picked once its answer comes', chip().textContent === lane.callsign + ' (main)' && !/Adding/.test(chip().title), chip().textContent + ' / ' + chip().title);
      var before = calls('start_from_quick').length;
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return calls('start_from_quick').length > before; }, 3000, 'the start').then(function () { return null; });
    })
    .then(function () {
      var spec = last('start_from_quick').spec;
      check('and the start goes there', spec.laneId === lane.id, spec.laneId);
      done();
    })
    .catch(stopped);
})();
