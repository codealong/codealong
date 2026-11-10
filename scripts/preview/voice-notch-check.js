// Checks what the bar docked in the notch shows of voice (src/said.ts, ui/Said.tsx): a key held
// to talk opens under the pills with the words as they're heard and to whom, without taking the
// keyboard, says where they went when it comes up, and goes away by itself; a session that
// listens shows the mark that it's hearing beside the notch's own, closed too; and a coach's cue
// opens the same way, is put away with a click, and stays away until the next one.
//   PREVIEW_SIZE=1512,720 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/voice-notch-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var card = function () { return q('.ca-notch-session .ca-spoken'); };
  var text = function (sel) { var el = q('.ca-notch-session ' + sel); return el ? el.textContent.trim() : ''; };
  var shownH = function () { var c = card(); return c ? Math.round(c.getBoundingClientRect().height) : 0; };
  // Whether the card is all inside the shape the notch shows, as it told the engine: the shape
  // is a clip over a layout made for a bigger one, so a card can be there and cut off.
  var inShape = function () {
    var c = card().getBoundingClientRect();
    var s = last('notch_shape');
    return c.left >= s.x - 1 && c.right <= s.x + s.w + 1 && c.bottom <= s.y + s.h + 1 ? '' : JSON.stringify({ card: [Math.round(c.left), Math.round(c.right), Math.round(c.bottom)], shape: s });
  };
  var talk = function (over) { window.__emit('talk', Object.assign({ id: 1, phase: 'listening', to: 'orchestrator', text: '', level: 0 }, over)); };
  var feed;
  var listening = function (over) {
    return Object.assign({ key: feed.items[0].key, title: 'Renewal call', id: 'a', startedAt: Date.now() - 247e3, role: 'coach', mic: true, playback: true, transcript: '', notes: '', cueAt: 0 }, over);
  };
  var hears = function (listenings) { window.__emit('bar-feed', Object.assign({}, feed, { listenings: listenings })); };

  say('voice notch check: waiting for it');
  until(function () { return notch() && q('.ca-notch-shape'); }, 20000, 'the notch')
    .then(function () { return fetch('./bar-feed.json').then(function (r) { return r.json(); }); })
    .then(function (f) {
      feed = Object.assign({}, f, { loading: false });
      check('has a session on the bar to try', feed.items.length > 0, String(feed.items.length));
      window.__emit('bar-feed', feed);
      return wait(300);
    })
    .then(function () {
      check('starts closed, with no mark that anything hears', !is('end') && !is('session') && !q('.ca-hearing-mark'));
      talk({ text: 'Check which sessions', level: 0.3 });
      return until(function () { return is('end') && card(); }, 3000, 'the card under the pills');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('a key held to talk opens under the pills', shownH() > 20, String(shownH()));
      check('all of it inside the notch’s shape', inShape() === '', inShape());
      check('saying it listens, and for whom', text('.ca-spoken-title') === 'Listening, for your orchestrator', text('.ca-spoken-title'));
      check('with the words heard so far', text('.ca-spoken-text') === 'Check which sessions', text('.ca-spoken-text'));
      check('and how loud', all('.ca-notch-session .ca-spoken-bar.is-on').length > 0, String(all('.ca-notch-session .ca-spoken-bar.is-on').length));
      check('without taking the keyboard', sent('notch_keys').filter(function (a) { return a.on; }).length === 0, JSON.stringify(sent('notch_keys')));
      talk({ text: 'Check which sessions are waiting on me.', level: 0.1 });
      return until(function () { return text('.ca-spoken-text') === 'Check which sessions are waiting on me.'; }, 2000, 'the words to grow');
    })
    .then(function () {
      talk({ phase: 'sent', title: 'Orchestrator', text: 'Check which sessions are waiting on me.' });
      return until(function () { return text('.ca-spoken-title') === 'Sent to Orchestrator'; }, 2000, 'where it went');
    })
    .then(function () {
      check('the key up, it says where the words went', text('.ca-spoken-text') === 'Check which sessions are waiting on me.', text('.ca-spoken-text'));
      return until(function () { return !card() && !is('end'); }, 5000, 'it to go away by itself');
    })
    .then(function () {
      check('and goes away by itself', !card());
      talk({ id: 2, phase: 'failed', to: 'looking', message: 'Codealong isn’t allowed to use the microphone. Allow it in System Settings › Privacy & Security › Microphone.' });
      return until(function () { return text('.ca-spoken-message').indexOf('isn’t allowed') >= 0; }, 2000, 'why it couldn’t listen');
    })
    .then(function () {
      check('one that couldn’t listen says why', text('.ca-spoken-title') === 'Couldn’t listen', text('.ca-spoken-title'));
      talk({ id: 3, phase: 'typed', to: 'looking', text: 'Hello.' });
      return until(function () { return !card(); }, 5000, 'the next one to go away');
    })
    .then(function () {
      // A session that listens: the mark, closed, and nothing opening for one with no cue.
      hears([listening({})]);
      return until(function () { return q('.ca-hearing-mark'); }, 2000, 'the mark that it hears');
    })
    .then(function (mark) {
      check('a session that listens is marked, closed too', !is('end') && !is('pills') && mark.getBoundingClientRect().width > 0);
      check('saying which', (mark.getAttribute('title') || '').indexOf('Renewal call is listening') === 0, mark.getAttribute('title'));
      hears([listening({ cue: 'Ask when they renew.\nNo budget owner yet.', cueAt: Date.now() })]);
      return until(function () { return is('end') && q('.ca-notch-session .ca-spoken.is-cue'); }, 3000, 'the cue');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('a coach’s cue opens under the pills', text('.ca-spoken-cue').indexOf('Ask when they renew.') === 0, text('.ca-spoken-cue'));
      check('all of it inside the notch’s shape too', inShape() === '', inShape());
      check('named for its session', text('.ca-spoken-title') === 'Renewal call', text('.ca-spoken-title'));
      check('without taking the keyboard either', sent('notch_keys').filter(function (a) { return a.on; }).length === 0);
      q('.ca-notch-session .ca-spoken.is-cue').click();
      return until(function () { return !card(); }, 2000, 'a click to put it away');
    })
    .then(function () {
      check('a click puts it away, and opens nothing else', !card() && !is('session') && !is('end') && !!q('.ca-hearing-mark'));
      // The same cue again doesn't bring it back; the next one does.
      hears([listening({ cue: 'Ask when they renew.\nNo budget owner yet.', cueAt: Date.now() - 500 })]);
      return wait(400);
    })
    .then(function () {
      check('and it stays away', !card());
      hears([listening({ cue: 'They said Q3, not Q2.', cueAt: Date.now() + 1000 })]);
      return until(function () { return text('.ca-spoken-cue') === 'They said Q3, not Q2.'; }, 3000, 'the next cue');
    })
    .then(function () {
      check('until the next cue', true);
      // A note-taker's line is no cue to open for.
      hears([listening({ role: 'notes', cue: 'Noted the date.', cueAt: Date.now() + 2000 })]);
      return until(function () { return !card(); }, 3000, 'a note-taker’s line not to show');
    })
    .then(function () {
      check('a note-taker’s lines open nothing', !card());
      hears([]);
      return until(function () { return !q('.ca-hearing-mark'); }, 2000, 'the mark to go');
    })
    .then(function () { check('the mark goes when nothing hears', true); })
    .then(done)
    .catch(stopped);
})();
