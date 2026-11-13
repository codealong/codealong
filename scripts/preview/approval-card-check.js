// Checks a call a session waits on you to allow, under its conversation in Sessions'
// panel (SessionReply.css, questionsBox.ts): the card that shows it is as wide as the message box
// under it, in the panel and expanded to the window, where both keep to the centered column; the
// message box has no rule or strip over it of its own; and the conversation has no line for the
// call the card shows, though it keeps one for another call of the same tool waiting at once with
// other input (an MCP tool's label is its name alone).
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/approval-card-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var label = 'mcp__tracker__create_issue';
  var first = '{"title":"Pin the toolchain"}';
  // The call its conversation ends on: the one the card shows, until another of the same tool
  // waits after it.
  var latest = first;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var mine = picked && (args || {}).key === picked.key;
    if (cmd === 'session_pending' && mine) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'ask-card', label: label, input: first, cannot: null });
    }
    if (cmd === 'session_page' && mine) {
      window.__calls.push([cmd, args, options]);
      var at = Date.now() - 10 * 60e3;
      var item = function (n, kind, text, extra) { return Object.assign({ at: at + n * 60e3, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({ before: null, after: 3, items: [
        item(0, 'you', 'File the toolchain issue'),
        item(1, 'agent', 'Filing it now.'),
        item(2, 'tool', label, { id: 'toolu_wait', input: latest }),
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = label;
          s.need = 'approval';
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var panel = function () { return q('.ca-sessinsp'); };
  var card = function () { return q('.ca-composer.is-approval', panel()); };
  var field = function () { return q('.ca-composer-field', panel()); };
  var thread = function () { return q('.ca-thread', panel()); };
  var line = function () { return q('.ca-thread-tool.is-asks', thread()); };
  var rect = function (el) { return el.getBoundingClientRect(); };
  var level = function (a, b) { return Math.abs(rect(a).left - rect(b).left) <= 1 && Math.abs(rect(a).right - rect(b).right) <= 1; };
  var sides = function () { return Math.round(rect(card()).left) + '–' + Math.round(rect(card()).right) + ' and ' + Math.round(rect(field()).left) + '–' + Math.round(rect(field()).right); };
  var expand = function () { return all('button', panel()).filter(function (b) { return /^(Expand to the window|Show the list)$/.test(b.getAttribute('aria-label') || ''); })[0]; };
  var reopen = function () {
    // Another session's row, then this one's again, reads its conversation again.
    var other = all('[id^="row-"]').filter(function (r) { return r !== row(); })[0];
    other.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return wait(300).then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return card() && thread() && /Filing it now/.test(thread().textContent); }, 5000, 'the session again');
    });
  };

  say('approval card check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return card() && field() && thread() && /Filing it now/.test(thread().textContent); }, 5000, 'the call to allow');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('the card says the call', new RegExp(label).test(card().textContent), card().textContent.slice(0, 80));
      check('no line for it in the conversation', !line(), line() ? line().textContent : '');
      check('the card is as wide as the message box', level(card(), field()), sides());
      var box = field().closest('.ca-composer');
      check('the message box has no rule of its own', getComputedStyle(box).borderTopWidth === '0px', getComputedStyle(box).borderTopWidth);
      check('the card ends above the message box', rect(card()).bottom <= rect(field()).top, Math.round(rect(card()).bottom) + ' > ' + Math.round(rect(field()).top));
      expand().click();
      return until(function () { return q('.ca-win-body.is-panel-max'); }, 3000, 'the panel expanded').then(function () { return wait(300); });
    })
    .then(function () {
      check('expanded, the message box keeps to the column', rect(field()).width <= 862, Math.round(rect(field()).width) + 'px');
      check('expanded, the card is as wide as the message box', level(card(), field()), sides());
      expand().click();
      // Another call of the same tool waits after it, with other input: the card shows the one
      // asked first, and the conversation's line for the latest stays.
      latest = '{"title":"Run the tests"}';
      return reopen();
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('another call with the same label keeps its line', !!line() && new RegExp(label).test(line().textContent), line() ? line().textContent : 'none');
    })
    .then(done)
    .catch(stopped);
})();
