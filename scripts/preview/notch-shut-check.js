// Checks what the closed notch counts beside the camera (Notch.tsx, bar.ts `notchShut`): the count
// of each kind that waits on you, side by side as they show open, and nothing else; the sides
// widen alike to fit them, never narrower than 44, so the counts that show only open lie past
// the closed shape's edge; opening it leaves them where they are and shows the rest after them;
// and with nothing waiting it counts what runs, at the narrowest.
//   PREVIEW_WAIT=40000 scripts/preview.sh notch '' scripts/preview/notch-shut-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var closed = function () { return all('.ca-notch-shut .ca-notch-count'); };
  var later = function () { return all('.ca-notch-side.is-right .ca-notch-side-in > .ca-notch-count'); };
  var words = function (els) { return els.map(function (c) { return (/is-(\w+)/.exec(c.className) || [])[1] + ' ' + c.textContent.trim(); }).join(', '); };
  var shown = function (el) { var s = getComputedStyle(el); return Number(s.opacity) > 0.99 && s.visibility === 'visible'; };
  var hidden = function (el) { var s = getComputedStyle(el); return Number(s.opacity) < 0.01 && s.visibility === 'hidden'; };
  // Where a count's glyph is: open, its box reaches out around it for the pointer, the glyph
  // stays put.
  var glyphAt = function (c) { return c.querySelector('svg').getBoundingClientRect().left; };
  var lastShape = function () { var s = sent('notch_shape'); return s[s.length - 1]; };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  var least = 185 + 2 * 44;

  // The bar with so many sessions in each group, all made from the feed's first, and so many PRs
  // waiting on your review.
  var base;
  var feed = fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (f) { base = f; return f; });
  var withCounts = function (needs, unread, working, reviews) {
    var first = base.items[0];
    var items = [];
    var add = function (group, n) {
      for (var i = 0; i < n; i++) items.push(Object.assign({}, first, { key: first.key + '-' + group + i, group: group, status: group, snoozedUntil: null, since: 1000 + items.length, hidden: false }));
    };
    add('needs', needs);
    add('unread', unread);
    add('working', working);
    var prs = [];
    for (var j = 0; j < reviews; j++) prs.push({ key: 'o/r#' + (j + 1), number: j + 1, title: 'A change', repo: 'r', repoSlug: 'o/r', url: 'https://github.com/o/r/pull/' + (j + 1), project: first.project, requestedBy: 'ana', since: 2000 + j, updatedAt: 2000 + j });
    window.__emit('bar-feed', Object.assign({}, base, { loading: false, items: items, recent: [], snoozed: [], reviews: prs, todos: [] }));
    // Past the pulse a count that needs you comes in with, which draws it bigger for a moment,
    // and the shape easing to its new width.
    return wait(1300);
  };
  // The closed shape fits the counts beside the camera, with room to spare, and once it's wider
  // than its least, the counts that show only open start past its edge.
  var fits = function () {
    var s = lastShape();
    var right = s.x + s.w;
    var last = closed()[closed().length - 1].getBoundingClientRect().right;
    var after = later().concat(all('.ca-notch-pin'));
    var past = s.w === least || after.every(function (c) { return c.getBoundingClientRect().left >= right - 1; });
    return { ok: s.w >= least && (s.w - 185) % 2 === 0 && last <= right - 5 && past, why: 'shape ' + s.x + '+' + s.w + ', counts to ' + Math.round(last) + ', the rest from ' + after.map(function (c) { return Math.round(c.getBoundingClientRect().left); }).join(' ') };
  };

  say('notch shut check: waiting for it');
  until(function () { return notch() && document.querySelector('.ca-notch-count'); }, 15000, 'the notch')
    .then(function () { return feed; })
    .then(function (f) {
      if (!f.items.length) throw new Error('the bar has no session to make others from');
      return withCounts(3, 7, 1, 0);
    })
    .then(function () {
      check('closed, each kind that waits has its own count', words(closed()) === 'needs 3, unread 7', words(closed()));
      check('shown, side by side', closed().every(shown) && closed()[1].getBoundingClientRect().left >= closed()[0].getBoundingClientRect().right, words(closed()));
      check('what runs shows only open', words(later()) === 'working 1' && later().every(hidden), words(later()));
      var f = fits();
      check('the sides widen to fit them, the rest past the edge', f.ok && lastShape().w > least, f.why);
      var at = closed().map(glyphAt);
      enter();
      return until(isOpen, 1500, 'open').then(function () { return wait(600); }).then(function () { return at; });
    })
    .then(function (at) {
      var now = closed().map(glyphAt);
      check('open, they stay where they were', now.every(function (x, i) { return Math.abs(x - at[i]) < 0.5; }), at.join() + ' then ' + now.join());
      check('and the rest show after them', later().every(shown) && words(later()) === 'working 1', words(later()));
      leave();
      return until(function () { return !isOpen(); }, 1500, 'closed').then(function () { return wait(600); });
    })
    .then(function () {
      check('closed again, the rest hide', later().every(hidden) && closed().every(shown));
      return withCounts(12, 31, 2, 40);
    })
    .then(function () {
      check('all three kinds with two digits', words(closed()) === 'needs 12, reviews 40, unread 31', words(closed()));
      var f = fits();
      check('fit, the rest past the edge', f.ok, f.why);
      return withCounts(0, 4, 2, 0);
    })
    .then(function () {
      check('one kind alone', words(closed()) === 'unread 4' && words(later()) === 'working 2', words(closed()) + ' / ' + words(later()));
      var f = fits();
      check('at the narrowest', f.ok && lastShape().w === least, f.why);
      return withCounts(0, 0, 3, 0);
    })
    .then(function () {
      check('nothing waiting: what runs', words(closed()) === 'working 3' && later().length === 0, words(closed()));
      check('at the narrowest too', lastShape().w === least, JSON.stringify(lastShape()));
    })
    .then(done)
    .catch(stopped);
})();
