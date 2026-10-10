// Checks that a PR's description keeps to its panel's width (PrInspector.css, panel.css), in the
// PRs view's panel and on its card in the waiting window: a long line of code wraps in its block,
// so neither the block nor anything around it scrolls sideways; a table wider than the panel
// scrolls in a box of its own, its words whole; and a narrow table keeps its own width.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-body-width-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/pr-body-width-check.js
(function () {
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var line = 'let total = ' + Array(30).join('someLongIdentifierName + ') + '1;';
  var cell = function (text) { return '<td>' + text + '</td>'; };
  var wide = '<table class="is-wide"><tr>' + [1, 2, 3, 4, 5, 6, 7, 8].map(function (n) { return cell('column_number_' + n + '_heading'); }).join('') + '</tr></table>';
  var narrow = '<table class="is-narrow"><tr>' + cell('a') + cell('5,7 / 28') + '</tr></table>';
  var BODY = '<p>Adds teams.</p><pre><code>' + line + '\nshort</code></pre>' +
    '<div class="highlight highlight-source-rust notranslate position-relative overflow-auto"><pre>fn main() { ' + line + ' }</pre></div>' +
    wide + narrow + '<p>After.</p>';
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail') {
      return Promise.resolve({ bodyHtml: BODY, mergeable: null, commits: 2, reviews: [], requested: [], comments: [], files: [], moreFiles: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      // In the waiting window, the first PR asks for your review and nothing else waits.
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        });
      });
      board.prs = (board.prs || []).map(function (x) {
        if (pr && x.key !== pr.key) return Object.assign({}, x, { reviewRequested: false });
        pr = Object.assign({}, x, { state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null });
        return pr;
      });
      return board;
    });
  };

  var body = function () { return q('.ca-prbody'); };
  // Every box from the description up that scrolls or spills sideways, by how far.
  var spills = function () {
    var out = [];
    for (var el = body(); el && el !== document.documentElement; el = el.parentElement) {
      if (el.scrollWidth > el.clientWidth + 1) out.push(el.tagName.toLowerCase() + '.' + String(el.className).split(' ').join('.') + ' ' + el.scrollWidth + ' in ' + el.clientWidth);
    }
    return out;
  };

  say('pr body width check: waiting for the description');
  until(function () { return q('.ca-prbody') || q('.ca-prrow'); }, 20000, 'a PR')
    .then(function () {
      if (!body()) press(q('.ca-prrow'));
      return until(function () { return q('.ca-prbody pre'); }, 5000, 'the description');
    })
    .then(function () {
      // A PR opens on its Review tab when an agent's review of it waits there: the description
      // is on Description.
      var tab = all('.ca-tab').filter(function (t) { return t.firstChild && t.firstChild.textContent === 'Description'; })[0];
      if (tab && tab.getAttribute('aria-selected') !== 'true') tab.click();
      return until(function () { return body().clientWidth > 0; }, 3000, 'the description never showed');
    })
    .then(function () {
      var s = spills();
      check('nothing from the description up scrolls sideways', s.length === 0, s.join('; '));
      var w = body().clientWidth;
      all('.ca-prbody pre').forEach(function (pre, i) {
        var lines = Math.round(pre.clientHeight / parseFloat(getComputedStyle(pre).lineHeight || '18'));
        check('code block ' + (i + 1) + ' wraps its long line within the description', pre.scrollWidth <= pre.clientWidth + 1 && pre.offsetWidth <= w + 1 && pre.clientHeight > 60, pre.scrollWidth + ' in ' + pre.clientWidth + ', ' + pre.clientHeight + 'px tall (~' + lines + ' lines)');
      });
      var t = q('.ca-prbody table.is-wide');
      check('a wide table scrolls in its own box, inside the description', t.scrollWidth > t.clientWidth && t.offsetWidth <= w + 1, t.scrollWidth + ' in ' + t.clientWidth + ', description ' + w);
      var tall = all('td', t).filter(function (td) { return td.clientHeight > 30; });
      check('its words stay whole', tall.length === 0, tall.map(function (td) { return td.textContent + ' ' + td.clientHeight + 'px'; }).join('; '));
      var n = q('.ca-prbody table.is-narrow');
      check('a narrow table keeps its own width', n.offsetWidth < w / 2 && n.scrollWidth <= n.clientWidth, n.offsetWidth + ' of ' + w);
    })
    .then(done, stopped);
})();
