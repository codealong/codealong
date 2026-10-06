// Checks a table in a reply: no word in a cell breaks across lines (a count, a header, a long
// `snake_case` name), and in a reply too narrow for it, the table scrolls sideways inside it
// rather than squeezing its columns or widening the thread. The conversation is a stand-in, on
// the first session:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/thread-table-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=thread-table) it leaves the panel open on it, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var TABLE = [
    'Why it’s blocked, from the run’s `report.json`:',
    '',
    '| Gap | Count | What it means |',
    '| --- | ---: | --- |',
    '| `incomplete_evaluation` | 5,728 | Nobody has evaluated the finding: reachability, exploitability, rating, impact, overdue status |',
    '| `missing_next_reduction_plan_or_reason` | 5,686 | No plan or reason recorded for reducing the finding |',
    '| `unverified_disposition` | 4,665 | A finding is closed, fixed or no longer seen, but nobody has verified it |',
    '| `container_coverage_or_continuity_failure` | 6 | Container scan snapshots from 09-23 through 09-28 |',
    '| `missing_start_baseline` | 1 | No snapshot at the start of the window |',
  ].join('\n');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n) { return { at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }; };
      return Promise.resolve({ before: null, after: 3, items: [
        item('you', 'Why is the report blocked?', 0),
        item('agent', TABLE, 1),
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var tables = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-md table')); };
  // Every word in the cells, with the lines it's drawn on: a word split by wrapping has more than
  // one. Text may wrap after a hyphen, so the parts of 09-28 count as words of their own.
  var split = function (table) {
    var out = [];
    var walk = document.createTreeWalker(table, NodeFilter.SHOW_TEXT);
    for (var node = walk.nextNode(); node; node = walk.nextNode()) {
      var re = /[^\s-]+/g;
      for (var m = re.exec(node.data); m; m = re.exec(node.data)) {
        var r = document.createRange();
        r.setStart(node, m.index);
        r.setEnd(node, m.index + m[0].length);
        var tops = {};
        Array.prototype.forEach.call(r.getClientRects(), function (rect) { if (rect.width > 0) tops[Math.round(rect.top)] = true; });
        if (Object.keys(tops).length > 1) out.push(m[0]);
      }
    }
    return out;
  };

  say('thread table check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return tables().length === 1; }, 10000);
    })
    .then(function () {
      var t = tables()[0];
      var box = t.closest('.ca-md-table');
      var reply = t.closest('.ca-thread-agent');
      note('reply ' + reply.clientWidth + ' wide, table ' + t.offsetWidth);
      var broken = split(t);
      check('no word in a cell breaks across lines', broken.length === 0, broken.join(' | '));
      check('the reply is no wider than the thread', reply.scrollWidth <= reply.clientWidth + 1, reply.scrollWidth + ' > ' + reply.clientWidth);
      // As in a narrow card, where its longest name alone is wider than the reply.
      reply.style.width = '260px';
      broken = split(t);
      check('narrowed, its words still stay whole', broken.length === 0, broken.join(' | '));
      check('and it scrolls sideways inside the reply', box.scrollWidth > box.clientWidth + 1 && getComputedStyle(box).overflowX === 'auto', box.scrollWidth + ' in ' + box.clientWidth);
      check('which it doesn’t widen', reply.scrollWidth <= reply.clientWidth + 1, reply.scrollWidth + ' > ' + reply.clientWidth);
      reply.style.width = '';
    })
    .then(done)
    .catch(stopped);
})();
