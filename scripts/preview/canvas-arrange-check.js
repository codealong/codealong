// Checks the main window's answers to `codealong canvas …` (canvas/useCanvasOps.ts), as the
// engine hands them over (canvas.rs `ask`): `show` says what the canvas holds; `arrange` lays out
// the frames it names, with their notes, says what it made and what the board lacks, and saves the
// canvas; a reveal brings the canvas up with those frames in view; a project the window doesn't
// show is arranged and saved without switching to it; a project there isn't is refused; and a
// save from another window has this one read the canvas again, where its own doesn't.
//   scripts/preview.sh canvas '' scripts/preview/canvas-arrange-check.js
(function () {
  var asked = 100;
  var ask = function (project, op) {
    var id = ++asked;
    window.__emit('canvas-ask', { id: id, project: project, op: op });
    return until(function () { return calls('canvas_answer').filter(function (c) { return c[1].id === id; })[0]; }, 5000, 'the answer to request ' + id).then(function (c) { return c[1]; });
  };
  var saved = function (project) {
    var c = calls('canvas_save').filter(function (c) { return !project || c[1].project === project; });
    return c.length ? c[c.length - 1][1] : null;
  };
  var within = function (outer, inner) {
    var cx = inner.x + inner.w / 2, cy = inner.y + inner.h / 2;
    return cx >= outer.x && cx <= outer.x + outer.w && cy >= outer.y && cy <= outer.y + outer.h;
  };

  until(function () { return saved(); }, 15000, 'the canvas, tidied and saved')
    .then(async function (first) {
      var project = first.project;
      var session = first.doc.items.filter(function (it) { return it.kind === 'ref' && it.ref === 'session'; })[0];
      if (!session) return stopped('no session on the ' + project + ' canvas');

      var shown = await ask(null, { kind: 'show' });
      check('show answers for the project the window shows', shown.result && shown.result.project === project && shown.result.exists, JSON.stringify(shown).slice(0, 160));
      check('show lists its cards with titles', shown.result && shown.result.cards.some(function (c) { return c.key === session.key && c.title; }));

      var made = await ask(null, {
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
      var doc = (await until(function () { var s = saved(project); return s && s.doc.items.some(function (it) { return it.id === frameId; }) && s; }, 3000, 'the arranged canvas saved')).doc;
      var frame = doc.items.filter(function (it) { return it.id === frameId; })[0];
      var card = doc.items.filter(function (it) { return it.kind === 'ref' && it.key === session.key; })[0];
      check('saved as an arranged frame, not a tidy’s', frame && frame.arranged === true && !frame.auto);
      check('holding the card it names', frame && card && within(frame, { x: card.x, y: card.y, w: 248, h: 56 }));

      window.__emit('reveal', { kind: 'canvas', project: project, focus: [frameId] });
      await wait(300);
      var box = title.parentElement.getBoundingClientRect();
      var view = q('.ca-cv').getBoundingClientRect();
      check('a reveal brings the frame into view', box.left >= view.left - 1 && box.right <= view.right + 1 && box.top >= view.top - 1 && box.bottom <= view.bottom + 1, [box.left, box.top, box.right, box.bottom, view.right, view.bottom].map(Math.round).join(' '));

      var projects = (window.__board && window.__board.projects) || [];
      var other = projects.map(function (p) { return p.name; }).filter(function (n) { return n !== project; })[0];
      if (other) {
        var there = await ask(other, { kind: 'arrange', frames: [{ title: 'Elsewhere note', note: 'Arranged from afar.', cards: [] }], rest: 'keep' });
        check('another project is arranged and saved', there.result && there.result.project === other && saved(other) && saved(other).doc.items.some(function (it) { return it.kind === 'note' && it.text === 'Arranged from afar.'; }), JSON.stringify(there).slice(0, 160));
        check('without the window leaving its project', all('.ca-cv-frame-title').some(function (el) { return el.textContent === 'Checked frame'; }));
      } else note('one project only: arranging another went unchecked');

      var wrong = await ask('No such project', { kind: 'show' });
      check('a project there isn’t is refused', wrong.error && wrong.error.indexOf("There's no project No such project") === 0, wrong.error);

      // A change of this window's own still to save (the camera the reveal moved) goes over another
      // window's save rather than reading it: wait for those to be saved first.
      for (var n = -1; n !== calls('canvas_save').length; ) {
        n = calls('canvas_save').length;
        await wait(900);
      }
      var loads = calls('canvas_load').length;
      window.__emit('canvas-saved', { project: project, from: 'main' });
      await wait(200);
      check('its own save isn’t read again', calls('canvas_load').length === loads);
      window.__emit('canvas-saved', { project: project, from: 'project-other' });
      await until(function () { return calls('canvas_load').length > loads; }, 3000, 'the canvas read again');
      check('another window’s save is read again', calls('canvas_load').length === loads + 1);

      // A read of another window's save that an arrange overtakes doesn't put back what came
      // before the arrange when it lands: the read is held until the arrange is saved.
      var invoke = window.__TAURI_INTERNALS__.invoke;
      var release = null;
      window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
        if (cmd === 'canvas_load' && args && args.project === project && !release) {
          window.__calls.push([cmd, args]);
          return new Promise(function (res) { release = function () { res({ version: 1, items: [], hidden: [], camera: null }); }; });
        }
        return invoke.apply(this, arguments);
      };
      window.__emit('canvas-saved', { project: project, from: 'project-other' });
      await until(function () { return release; }, 3000, 'the read held');
      var overtaking = await ask(null, { kind: 'arrange', frames: [{ title: 'After the read', cards: [{ ref: 'session', key: session.key }] }], rest: 'keep' });
      release();
      await wait(500);
      window.__TAURI_INTERNALS__.invoke = invoke;
      check('a read an arrange overtook leaves the arrangement', !overtaking.error && all('.ca-cv-frame-title').some(function (el) { return el.textContent === 'After the read'; }));
    })
    .then(done)
    .catch(stopped);

  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  window.__board = JSON.parse(xhr.responseText);
})();
