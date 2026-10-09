// Checks the one count beside the closed notch's camera (Notch.tsx, bar.ts `notchSum`): what
// waits on you of more than one kind shows closed as one count, the total, drawn as each kind's
// glyph side by side, none over another, and saying each count in its tooltip, within the closed
// shape even with all three kinds and two digits; opening it shows each count in its place; and
// one kind alone is its own count closed, as before.
//   PREVIEW_WAIT=40000 scripts/preview.sh notch '' scripts/preview/notch-sum-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var sum = function () { return document.querySelector('.ca-notch-sum'); };
  var counts = function () { return all('.ca-notch-side.is-right button.ca-notch-count'); };
  var shown = function (el) { return el && Number(getComputedStyle(el).opacity) > 0.99; };
  var hidden = function (el) { return el && Number(getComputedStyle(el).opacity) < 0.01; };
  var lastShape = function () { var s = sent('notch_shape'); return s[s.length - 1]; };
  var closedW = 185 + 2 * 66;
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };

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
    // Past the pulse it comes in with, which draws it bigger for a moment.
    return wait(1300);
  };
  // The sum's glyphs, front first, by what they draw.
  var glyphs = function () {
    return all('.ca-waitsglyphs > *', sum()).map(function (g) {
      return g.classList.contains('ca-glyph-review') ? 'reviews' : (/ca-glyph--(\w+)/.exec(g.getAttribute('class')) || [])[1];
    }).join(',');
  };

  // Where each glyph is, and whether each starts after the one before it ends, give or take the
  // point each takes of the other's room.
  var spans = function () { return all('.ca-waitsglyphs > *', sum()).map(function (g) { var b = g.getBoundingClientRect(); return Math.round(b.left) + '–' + Math.round(b.right); }).join(' '); };
  var apart = function () {
    var parts = all('.ca-waitsglyphs > *', sum()).map(function (g) { return g.getBoundingClientRect(); });
    return parts.every(function (p, i) { return i === 0 || p.left >= parts[i - 1].right - 1.5; });
  };

  say('notch sum check: waiting for it');
  until(function () { return notch() && document.querySelector('.ca-notch-count'); }, 15000, 'the notch')
    .then(function () { return feed; })
    .then(function (f) {
      if (!f.items.length) throw new Error('the bar has no session to make others from');
      return withCounts(3, 7, 1, 0);
    })
    .then(function () {
      check('closed, what waits shows as one count', shown(sum()) && sum().textContent.trim() === '10', sum() && sum().textContent);
      check('drawn as the glyph of each kind in it', glyphs() === 'needs,unread', glyphs());
      check('its tooltip says each', sum().title === '3 need you, 7 your turn', sum().title);
      check('the counts it adds up hide', counts().every(hidden), counts().map(function (c) { return getComputedStyle(c).opacity; }).join());
      var r = sum().getBoundingClientRect();
      var s = lastShape();
      check('within the closed shape', s.w === closedW && r.right <= s.x + s.w - 2, Math.round(r.right) + ' of ' + (s.x + s.w));
      check('none drawn over another', apart(), spans());
      enter();
      return until(isOpen, 1500, 'open').then(function () { return wait(600); });
    })
    .then(function () {
      check('open, the one count goes', hidden(sum()) && getComputedStyle(sum()).pointerEvents === 'none');
      var words = counts().map(function (c) { return c.textContent.trim(); }).join(' ');
      check('and each count shows in its place', counts().every(shown) && words === '3 7 1', words);
      var first = counts()[0].getBoundingClientRect();
      check('the first where the one count was', Math.abs(first.left - sum().getBoundingClientRect().left) < 6, Math.round(first.left) + ' and ' + Math.round(sum().getBoundingClientRect().left));
      leave();
      return until(function () { return !isOpen(); }, 1500, 'closed').then(function () { return wait(600); });
    })
    .then(function () {
      check('closed again, the one count is back', shown(sum()) && counts().every(hidden));
      return withCounts(0, 4, 2, 0);
    })
    .then(function () {
      check('one kind alone is its own count', !sum() && shown(counts()[0]) && counts()[0].textContent.trim() === '4', counts()[0] && counts()[0].textContent);
      return withCounts(0, 2, 0, 3);
    })
    .then(function () {
      check('a review and turns that ended', shown(sum()) && glyphs() === 'reviews,unread' && sum().textContent.trim() === '5', glyphs() + ' ' + (sum() && sum().textContent));
      check('saying the reviews first', sum().title === '3 PRs wait on your review, 2 your turn', sum().title);
      return withCounts(12, 31, 0, 40);
    })
    .then(function () {
      var r = sum().getBoundingClientRect();
      var s = lastShape();
      check('all three kinds, side by side', glyphs() === 'needs,reviews,unread' && apart(), glyphs() + ' ' + spans());
      check('and two digits fit the closed shape', sum().textContent.trim() === '83' && s.w === closedW && r.right <= s.x + s.w - 4, Math.round(r.right) + ' of ' + (s.x + s.w));
    })
    .then(done)
    .catch(stopped);
})();
