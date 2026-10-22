// Checks the main window's answers to `codealong board show --as canvas` and `board arrange`
// (boards/useBoardAsks.ts), as the engine hands them over (boards.rs `ask`): `show` says what a
// board's canvas holds; `arrange` lays out the frames it names, with their notes, says what it
// made and what the project lacks, and saves the canvas; a reveal brings the canvas up with those
// frames in view; another project's board is arranged and saved without switching to it; a board
// there isn't is refused; a save from another window has this one read the canvas again, where
// its own doesn't; and with the rest off, what no frame names comes off the board too, and the
// window doesn't put it back meanwhile.
// Each project has a board that takes in everything, boards-everything.json's.
//   PREVIEW_WAIT=90000 PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh canvas '' scripts/preview/canvas-arrange-check.js
(function () {
  var asked = 100;
  // A project's board that takes in everything, by the project.
  var EVERY = 'b-all-';
  var own = function (project) { return EVERY + project; };
  var ask = function (board, op) {
    var id = ++asked;
    window.__emit('board-ask', { id: id, board: board, project: board.indexOf(EVERY) === 0 ? board.slice(EVERY.length) : '', ask: op });
    return until(function () { return calls('board_answer').filter(function (c) { return c[1].id === id; })[0]; }, 5000, 'the answer to request ' + id).then(function (c) { return c[1]; });
  };
  var saved = function (board) {
    var c = calls('canvas_save').filter(function (c) { return !board || c[1].board === board; });
    return c.length ? c[c.length - 1][1] : null;
  };
  var SHOW = { kind: 'show', layout: 'canvas' };
  // Holds the next call of a command until release(), for what happens in the window meanwhile.
  var hold = function (cmd) {
    var real = window.__TAURI_INTERNALS__.invoke;
    var held = null;
    window.__TAURI_INTERNALS__.invoke = function (c) {
      if (c !== cmd || held) return real.apply(this, arguments);
      var got = real.apply(this, arguments);
      return new Promise(function (res) { held = function () { window.__TAURI_INTERNALS__.invoke = real; res(got); }; });
    };
    return { waiting: function () { return !!held; }, release: function () { held(); } };
  };
  var within = function (outer, inner) {
    var cx = inner.x + inner.w / 2, cy = inner.y + inner.h / 2;
    return cx >= outer.x && cx <= outer.x + outer.w && cy >= outer.y && cy <= outer.y + outer.h;
  };

  until(function () { return saved(); }, 15000, 'the canvas, tidied and saved')
    .then(async function (first) {
      var board = first.board;
      var project = board.slice(EVERY.length);
      var session = first.doc.items.filter(function (it) { return it.kind === 'ref' && it.ref === 'session'; })[0];
      if (!session) return stopped('no session on the ' + project + ' canvas');

      var shown = await ask(board, SHOW);
      check('show answers for the board the window shows', shown.result && shown.result.board.id === board && shown.result.exists, JSON.stringify(shown).slice(0, 160));
      check('show lists its cards with titles', shown.result && shown.result.cards.some(function (c) { return c.key === session.key && c.title; }));

      var made = await ask(board, {
        kind: 'arrange',
        frames: [
          { title: 'Checked frame', note: 'Made by the canvas check.', cards: [{ ref: 'session', key: session.key }] },
          { title: 'Nothing here', cards: [{ ref: 'session', key: 'claude:nobody:none' }] },
        ],
        rest: 'keep',
      });
      var r = made.result || {};
      check('arrange says the frame it made', r.frames && r.frames.length === 1 && r.frames[0].title === 'Checked frame' && r.frames[0].cards === 1, JSON.stringify(made).slice(0, 200));
      check('and what the board lacks', r.missing && r.missing.length === 1 && r.missing[0].key === 'claude:nobody:none');
      var frameId = r.frames && r.frames[0] && r.frames[0].id;
      var title = await until(function () { return all('.ca-cv-frame-title').filter(function (el) { return el.textContent === 'Checked frame'; })[0]; }, 3000, 'the frame drawn');
      check('the frame is drawn', !!title);
      check('with its note', all('.ca-cv-note-text').some(function (el) { return el.textContent === 'Made by the canvas check.'; }));
      var doc = (await until(function () { var s = saved(board); return s && s.doc.items.some(function (it) { return it.id === frameId; }) && s; }, 3000, 'the arranged canvas saved')).doc;
      var frame = doc.items.filter(function (it) { return it.id === frameId; })[0];
      var card = doc.items.filter(function (it) { return it.kind === 'ref' && it.key === session.key; })[0];
      check('saved as an arranged frame, not a tidy’s', frame && frame.arranged === true && !frame.auto);
      check('holding the card it names', frame && card && within(frame, { x: card.x, y: card.y, w: 248, h: 56 }));

      window.__emit('reveal', { kind: 'board', board: board, project: project, layout: 'canvas', focus: [frameId] });
      await wait(300);
      var box = title.parentElement.getBoundingClientRect();
      var view = q('.ca-cv').getBoundingClientRect();
      check('a reveal brings the frame into view', box.left >= view.left - 1 && box.right <= view.right + 1 && box.top >= view.top - 1 && box.bottom <= view.bottom + 1, [box.left, box.top, box.right, box.bottom, view.right, view.bottom].map(Math.round).join(' '));

      var projects = (window.__board && window.__board.projects) || [];
      var other = projects.map(function (p) { return p.name; }).filter(function (n) { return n !== project; })[0];
      if (other) {
        var there = await ask(own(other), { kind: 'arrange', frames: [{ title: 'Elsewhere note', note: 'Arranged from afar.', cards: [] }], rest: 'keep' });
        check('another project’s board is arranged and saved', there.result && there.result.board.project === other && saved(own(other)) && saved(own(other)).doc.items.some(function (it) { return it.kind === 'note' && it.text === 'Arranged from afar.'; }), JSON.stringify(there).slice(0, 160));
        check('without the window leaving its project', all('.ca-cv-frame-title').some(function (el) { return el.textContent === 'Checked frame'; }));
      } else note('one project only: arranging another went unchecked');

      // The board as the engine has it when asked, not as the window last drew it (a window that
      // isn't drawn, hidden or behind a locked screen, has an old one): a session only the engine
      // has yet is arranged, not missing.
      var home = window.__board.lanes.filter(function (l) { return l.sessions.some(function (s) { return s.key === session.key; }); })[0];
      var freshKey = 'claude:claude:fresh-from-engine';
      var engineInvoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd) {
        var got = engineInvoke.apply(this, arguments);
        if (cmd !== 'get_board') return got;
        window.__TAURI_INTERNALS__.invoke = engineInvoke;
        return got.then(function (b) {
          var lane = b.lanes.filter(function (l) { return l.id === home.id; })[0];
          var copy = Object.assign({}, lane.sessions.filter(function (s) { return s.key === session.key; })[0], { key: freshKey, id: 'fresh-from-engine', title: 'Fresh from the engine' });
          lane.sessions = lane.sessions.concat([copy]);
          return b;
        });
      };
      var fresh = await ask(board, { kind: 'arrange', frames: [{ title: 'Fresh', cards: [{ ref: 'session', key: freshKey }] }], rest: 'keep' });
      window.__TAURI_INTERNALS__.invoke = engineInvoke;
      check('a request reads the board when it’s asked', fresh.result && fresh.result.missing.length === 0 && fresh.result.frames[0].cards === 1, JSON.stringify(fresh).slice(0, 200));

      var wrong = await ask('b-no-such-board', SHOW);
      check('a board there isn’t is refused', wrong.error && wrong.error.indexOf("There's no board b-no-such-board") === 0, wrong.error);

      // A change of this window's own still to save (the camera the reveal moved) goes over another
      // window's save rather than reading it: wait for those to be saved first.
      for (var n = -1; n !== calls('canvas_save').length; ) {
        n = calls('canvas_save').length;
        await wait(900);
      }
      var loads = calls('canvas_load').length;
      window.__emit('canvas-saved', { board: board, from: 'main' });
      await wait(200);
      check('its own save isn’t read again', calls('canvas_load').length === loads);
      window.__emit('canvas-saved', { board: board, from: 'project-other' });
      await until(function () { return calls('canvas_load').length > loads; }, 3000, 'the canvas read again');
      check('another window’s save is read again', calls('canvas_load').length === loads + 1);

      // A read of another window's save that an arrange overtakes doesn't put back what came
      // before the arrange when it lands: the read is held until the arrange is saved.
      var invoke = window.__TAURI_INTERNALS__.invoke;
      var release = null;
      window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
        if (cmd === 'canvas_load' && args && args.board === board && !release) {
          window.__calls.push([cmd, args]);
          return new Promise(function (res) { release = function () { res({ version: 1, items: [], camera: null }); }; });
        }
        return invoke.apply(this, arguments);
      };
      window.__emit('canvas-saved', { board: board, from: 'project-other' });
      await until(function () { return release; }, 3000, 'the read held');
      var overtaking = await ask(board, { kind: 'arrange', frames: [{ title: 'After the read', cards: [{ ref: 'session', key: session.key }] }], rest: 'keep' });
      release();
      await wait(500);
      window.__TAURI_INTERNALS__.invoke = invoke;
      check('a read an arrange overtook leaves the arrangement', !overtaking.error && all('.ca-cv-frame-title').some(function (el) { return el.textContent === 'After the read'; }));

      // A note added while an arrange reads the board is in what's arranged.
      var userNotes = function () { var s = saved(board); return s ? s.doc.items.filter(function (it) { return it.kind === 'note' && !it.arranged; }).length : 0; };
      var notesBefore = userNotes();
      var boardRead = hold('get_board');
      var noting = ask(board, { kind: 'arrange', frames: [{ title: 'With the note', cards: [{ ref: 'session', key: session.key }] }], rest: 'keep' });
      await until(boardRead.waiting, 3000, 'the board read held');
      var corner = q('.ca-cv').getBoundingClientRect();
      await menuOf(q('.ca-cv'), { clientX: corner.left + 8, clientY: corner.top + 8 });
      menuItem('Add note here')();
      await wait(200);
      boardRead.release();
      var noted = await noting;
      await wait(300);
      check('a note added while the board was read is kept', !noted.error && userNotes() === notesBefore + 1, notesBefore + ' then ' + userNotes());

      // An arrange that the window moved on to another project from while it was out saves its
      // project's canvas and leaves the one the window shows.
      if (other) {
        var away = hold('get_board');
        var leaving = ask(board, { kind: 'arrange', frames: [{ title: 'While away', cards: [{ ref: 'session', key: session.key }] }], rest: 'keep' });
        await until(away.waiting, 3000, 'the board read held');
        window.__emit('reveal', { kind: 'board', board: own(other), project: other, layout: 'canvas' });
        await wait(1500);
        away.release();
        var left = await leaving;
        await wait(500);
        check('an arrange the window moved on from is saved for its board', !left.error && saved(board).doc.items.some(function (it) { return it.kind === 'frame' && it.title === 'While away'; }), JSON.stringify(left).slice(0, 160));
        check('and leaves the canvas the window shows now', !!q('.ca-cv') && !all('.ca-cv-frame-title').some(function (el) { return el.textContent === 'While away'; }));

        // An arrange whose read of the file is out while a note is added and the window leaves
        // the project (which saves the note) reads the file again. The stub's reads answer from
        // the last save, as the file would, and the arrange's first is held.
        var stub = window.__TAURI_INTERNALS__.invoke;
        var heldRead = null;
        var armed = false;
        window.__TAURI_INTERNALS__.invoke = function (c, args) {
          if (c !== 'canvas_load') return stub.apply(this, arguments);
          window.__calls.push([c, args]);
          var file = saved(args.board) ? saved(args.board).doc : null;
          if (!armed || args.board !== board || heldRead) return Promise.resolve(file);
          return new Promise(function (res) { heldRead = function () { res(file); }; });
        };
        window.__emit('reveal', { kind: 'board', board: board, project: project, layout: 'canvas' });
        await wait(1500);
        var notesThen = userNotes();
        armed = true;
        var reading = ask(board, { kind: 'arrange', frames: [{ title: 'Read again', cards: [{ ref: 'session', key: session.key }] }], rest: 'keep' });
        await until(function () { return heldRead; }, 3000, 'the file read held');
        corner = q('.ca-cv').getBoundingClientRect();
        await menuOf(q('.ca-cv'), { clientX: corner.left + 8, clientY: corner.top + 8 });
        menuItem('Add note here')();
        await wait(200);
        window.__emit('reveal', { kind: 'board', board: own(other), project: other, layout: 'canvas' });
        await wait(1500);
        heldRead();
        var readAgain = await reading;
        await wait(300);
        window.__TAURI_INTERNALS__.invoke = stub;
        check('a read the window’s save overtook is read again', !readAgain.error && userNotes() === notesThen + 1 && saved(board).doc.items.some(function (it) { return it.kind === 'frame' && it.title === 'Read again'; }), notesThen + ' then ' + userNotes());
        window.__emit('reveal', { kind: 'board', board: board, project: project, layout: 'canvas' });
        await wait(1500);
      }

      // With the rest off, what no frame names comes off the canvas and the board together: the
      // window, which shows the new canvas while it's still being saved, doesn't put the rest
      // back (the board would show it still) and grow the frame round it. The arrange's save is
      // held to see the window meanwhile, once the window's own saves are done.
      for (var m = -1; m !== calls('canvas_save').length; ) {
        m = calls('canvas_save').length;
        await wait(900);
      }
      var cardsOn = function () { return saved(board).doc.items.filter(function (it) { return it.kind === 'ref'; }).map(function (it) { return it.key; }).join(', '); };
      var saving = hold('canvas_save');
      var justThis = ask(board, { kind: 'arrange', frames: [{ title: 'Just this', cards: [{ ref: 'session', key: session.key }] }], rest: 'off' });
      await until(saving.waiting, 3000, 'the arranged canvas’s save held');
      await wait(1000);
      var removed = last('board_op');
      check('rest off takes the rest off the board', removed && removed.op.op === 'remove' && removed.op.cards.length > 0, JSON.stringify(removed).slice(0, 160));
      check('and the window doesn’t put it back on the canvas meanwhile', cardsOn() === session.key, cardsOn());
      saving.release();
      var offDone = await justThis;
      await wait(1000);
      check('nor once the board has it', !offDone.error && cardsOn() === session.key, cardsOn());
    })
    .then(done)
    .catch(stopped);

  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  window.__board = JSON.parse(xhr.responseText);
})();
