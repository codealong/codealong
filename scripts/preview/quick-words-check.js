// Checks what the quick panel says of what ⏎ would do (start.ts, route.ts, quick.ts): the start
// row's title and line, the agent chip and the New worktree box, Jev's suggestions for a prompt
// (their titles, lines, places and tooltips), and the agent menu's lines. It notes every one of
// them, so two builds run on one snapshot can be told apart by their notes.
//   PREVIEW_ROUTE=scripts/preview/route-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-words-check.js
//   PREVIEW_ROUTE=scripts/preview/route-start-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-words-check.js
(function () {
  var text = function (sel, root) { var el = q(sel, root); return el ? el.textContent : null; };
  var compose = function () { return q('.ca-quick-compose'); };
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var suggestions = function () { return all('.ca-quick-row.is-suggest'); };
  var box = function () { return q('.ca-quick-input textarea'); };

  localStorage.setItem('codealong.quick.project', 'All projects');
  say('quick words check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && !!agentChip() && !agentChip().disabled && !!compose(); }, 30000)
    .then(function () {
      check('the start row says what it does', !!text('.ca-quick-row-title', compose()) && !!text('.ca-quick-row-why', compose()), text('.ca-quick-row-why', compose()));
      note('start: ' + text('.ca-quick-row-title', compose()) + ' | ' + text('.ca-quick-row-why', compose()));
      note('agent chip: ' + agentChip().textContent);
      var tree = q('.ca-quick-newtree');
      note('new worktree: ' + (tree ? tree.title : 'none'));
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), 'Fix the login page so it remembers the account');
      box().dispatchEvent(new Event('input', { bubbles: true }));
      return until(function () { return suggestions().length > 0; }, 10000);
    })
    .then(function () {
      check('Jev’s suggestions lead the list', suggestions().length > 0, String(suggestions().length));
      suggestions().forEach(function (r, i) {
        note('suggestion ' + (i + 1) + ': ' + text('.ca-quick-row-title', r) + ' | ' + text('.ca-quick-row-why', r) + ' | ' + text('.ca-quick-row-where', r) + ' | ' + r.title.replace(/\n/g, ' / '));
      });
      note('start with a prompt: ' + text('.ca-quick-row-title', compose()) + ' | ' + text('.ca-quick-row-why', compose()) + ' | ' + text('.ca-quick-row-where', compose()));
      // The start row says its start as the suggestions say theirs, in the same columns, so the
      // two can be read against each other.
      var where = q('.ca-quick-row-where', compose());
      var theirs = q('.ca-quick-row-where', suggestions()[0]);
      check('the start row says where, as the suggestions do', !!where && !!where.textContent, where && where.textContent);
      if (where && theirs) {
        var edge = function (el) { return Math.round(el.getBoundingClientRect().right); };
        check('in the suggestions’ column', Math.abs(edge(where) - edge(theirs)) <= 1, edge(where) + ' vs ' + edge(theirs));
      }
      key(window, { key: 'l', metaKey: true });
      return until(function () { return all('.ca-picker-opt').length > 0; }, 3000);
    })
    .then(function () {
      var lines = all('.ca-picker-opt').map(function (o) { return o.textContent; });
      check('the agent menu lists its lines', lines.length > 0, String(lines.length));
      note('agent menu: ' + lines.join(' | '));
      var heads = all('.ca-quick-acct').map(function (h) { return h.textContent; });
      note('accounts: ' + heads.join(' | '));
      var foot = q('.ca-picker-foot-note');
      note('effort: ' + (foot ? foot.textContent : 'none'));
    })
    .then(done)
    .catch(stopped);
})();
