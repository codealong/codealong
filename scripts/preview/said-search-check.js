// Checks finding sessions by what was said in them (useTextSearch.ts, boardLists.ts, quick.ts).
// In the main window: words no session's title or details have, but one session's conversation
// does, list that session in Sessions with the line that has them, the words marked, and no other
// session; in any order. Selecting it shows its conversation with the find bar open on the words
// at a match, the keyboard left where it was; the words in another order find the longest of them
// its line has. Clearing the search takes the line away. A session the quick panel shows (`reveal`
// with `find`) opens with the bar on those words too, and again after the bar was closed when it's
// shown once more, until another session is selected. A PR one of whose sessions said the words is
// found in PRs, and that session shown in the PR's place opens with the bar on them. A search typed
// here retires what the quick panel asked: closing the bar and clearing the search doesn't bring
// it back. With
// PREVIEW_DEVICES, another device's session its own search found is listed as well. In the quick
// panel: ⏎ on a session found that way asks the main window to show it with the words.
//   scripts/preview.sh sessions '' scripts/preview/said-search-check.js
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/said-search-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/said-search-check.js
// The board needs two sessions in worktrees that aren't hidden.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.quick.project', 'All projects');
  var now = Date.now();
  var LINE = 'You said the Quokka bridge was out, so take the ferry.';
  var A = null;
  var B = null;
  var D = null;
  var P = null;
  var page = function (texts) {
    return {
      items: texts.map(function (t, i) { return { at: now - (10 - i) * 60e3, kind: i % 2 ? 'agent' : 'you', text: t, failed: false, cursor: i, n: 0 }; }),
      before: null,
      after: texts.length,
    };
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var said = function (words) { return words.indexOf('quokka') >= 0 || words.indexOf('bridge') >= 0; };
    if (cmd === 'search_text') {
      window.__calls.push([cmd, args, options]);
      var mine = said(args.words) && A ? [{ key: A.key, snippet: LINE, you: true, at: now, score: 6 }] : [];
      return Promise.resolve({ hits: mine, building: false, off: false });
    }
    if (cmd === 'search_devices') {
      window.__calls.push([cmd, args, options]);
      if (!D || !said(args.words)) return Promise.resolve([]);
      return Promise.resolve([{ device: D.short, name: 'Mac mini', found: { hits: [{ key: D.key, snippet: 'The quokka bridge on the other Mac.', you: false, at: now, score: 4 }], building: false, off: false }, error: null }]);
    }
    if (cmd === 'session_page' && ((A && args.key === A.key) || (B && args.key === B.key))) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(args.key === A.key ? page(['Which way to town?', 'The quokka bridge is the short way.', 'And now?', LINE]) : page(['Where is the lighthouse?', 'Past the lighthouse, on the left.']));
    }
    var p = real(cmd, args, options);
    if (cmd === 'devices')
      return p.then(function (view) {
        (view.lanes || []).forEach(function (l) {
          var s = l.hidden ? null : l.sessions.filter(function (x) { return !x.visitor && !x.archived; })[0];
          if (!D && s) D = { key: s.key, lane: l.id, short: l.device.short };
        });
        return view;
      });
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          var mine = (A && A.key === s.key) || (B && B.key === s.key);
          if (!mine && (A && B || l.hidden || s.visitor || s.archived)) return;
          s.title = !A || A.key === s.key ? 'Said A' : 'Said B';
          s.lastActivity = now;
          if (!A) A = { key: s.key, lane: l.id };
          else if (!B && A.key !== s.key) B = { key: s.key, lane: l.id };
        });
      });
      // A PR A worked on, with nothing of the words in what describes it.
      board.prs = (board.prs || []).map(function (pr, i) {
        if (P ? pr.key !== P.key : i > 0) return pr;
        P = { key: pr.key };
        return Object.assign({}, pr, { title: 'Tidy the trip', state: 'open', updatedAt: now, sessions: [{ lane: A.lane, key: A.key }] });
      });
      return board;
    });
  };

  var quick = function () { return !!q('.ca-quick-input textarea'); };
  var filter = function () { return q('.ca-search input'); };
  var rowOf = function (who) { return q('#row-' + CSS.escape(who.lane + '::' + who.key)); };
  var listed = function () { return all('.ca-sessionlist .ca-lane.is-agent'); };
  var saidIn = function (row) { return row ? q('.ca-agentrow-said', row) : null; };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var bar = function () { return q('.ca-thread-find'); };
  var field = function () { return q('.ca-thread-find input'); };
  var at = function () { var h = CSS.highlights.get('ca-find-at'); var r = h ? Array.from(h)[0] : null; return r ? r.toString() : null; };
  var shown = function (name) {
    return until(function () { var t = q('.ca-sessinsp .ca-thread'); return title() === name && !!t && !/Reading the conversation/.test(t.textContent); }, 5000).then(function () { return wait(300); });
  };
  var pick = function (who, name) {
    rowOf(who).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return shown(name);
  };

  var main = function () {
    return until(function () { return !!A && !!B && !!filter() && !!rowOf(A) && !!rowOf(B); }, 20000, 'two sessions in Sessions')
      .then(function () {
        type(filter(), 'quokka bridge');
        return until(function () { return !!saidIn(rowOf(A)); }, 5000, 'the line on its row');
      })
      .then(function () {
        var line = saidIn(rowOf(A));
        var marks = all('mark', line).map(function (m) { return m.textContent; });
        check('the search finds the session by what was said in it', !!rowOf(A) && !rowOf(B), listed().length + ' listed');
        check('its row shows the line, who said it and the words', line.textContent === 'You: ' + LINE, line.textContent);
        check('the words are marked', marks.join('|') === 'Quokka|bridge', marks.join('|'));
        var words = sent('search_text').pop();
        check('it asked the text search for the words', !!words && words.words.join(' ') === 'quokka bridge', JSON.stringify(words));
        if (D) {
          check('another device’s session its search found is listed', !!rowOf(D) && !!saidIn(rowOf(D)), D.key);
          check('and the device was asked', sent('search_devices').length > 0);
        } else note('no other device (PREVIEW_DEVICES): its sessions weren’t searched');
        return pick(A, 'Said A');
      })
      .then(function () { return until(function () { return !!field() && !!at(); }, 5000, 'the find bar at a match').catch(function () {}); })
      .then(function () {
        check('its conversation shows with the find bar open on the words', !!bar() && field().value === 'quokka bridge', field() && field().value);
        check('at a match', /^quokka bridge$/i.test(at() || ''), at());
        check('the keyboard stays where it was', document.activeElement !== field(), document.activeElement && document.activeElement.className);
        type(filter(), 'bridge quokka');
        return until(function () { return field() && field().value === 'bridge'; }, 5000).catch(function () {});
      })
      .then(function () {
        check('the words in another order still find it', !!saidIn(rowOf(A)));
        check('and the bar finds the longest of them its line has', field() && field().value === 'bridge', field() && field().value);
        type(filter(), '');
        return until(function () { return !q('.ca-agentrow-said') && !!rowOf(B); }, 5000).catch(function () {});
      })
      .then(function () {
        check('clearing the search takes the line away', !q('.ca-agentrow-said') && !!rowOf(B));
        // As the quick panel asks it: B, found by "lighthouse".
        window.__emit('reveal', { kind: 'session', laneId: B.lane, key: B.key, find: 'lighthouse' });
        return shown('Said B');
      })
      .then(function () { return until(function () { return !!field() && !!at(); }, 5000).catch(function () {}); })
      .then(function () {
        check('a session the quick panel shows opens with the bar on its words', !!field() && field().value === 'lighthouse' && /^lighthouse$/i.test(at() || ''), (field() && field().value) + ' / ' + at());
        check('without the keyboard', document.activeElement !== field());
        q('.ca-thread-find button[aria-label="Close find"]').click();
        return until(function () { return !bar(); }, 2000).then(function () {
          window.__emit('reveal', { kind: 'session', laneId: B.lane, key: B.key, find: 'lighthouse' });
          return until(function () { return !!field(); }, 3000).catch(function () {});
        });
      })
      .then(function () {
        check('shown once more, it opens the bar it had closed', !!field() && field().value === 'lighthouse');
        return pick(A, 'Said A');
      })
      .then(function () { return pick(B, 'Said B'); })
      .then(function () {
        check('another session selected, it opens without the bar again', !bar());
        if (!P) return note('no PR on the board: a PR found by what its sessions said wasn’t checked');
        key(document.body, { key: 'P', code: 'KeyP', metaKey: true, shiftKey: true });
        type(filter(), 'quokka bridge');
        var prRow = function () { return document.getElementById('row-pr:' + P.key); };
        return until(function () { return !!prRow(); }, 5000, 'the PR in PRs')
          .then(function () {
            check('a PR is found by what a session that worked on it said', !!prRow());
            prRow().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
            // Its chip among the PR's places, or its row once the details are open.
            return until(function () {
              return all('.ca-prpanel .ca-sesschip, .ca-prpanel .ca-psess-main').filter(function (b) { var t = q('.ca-sesschip-title, .ca-psess-title', b); return !!t && t.textContent === 'Said A'; })[0];
            }, 5000, 'A in the PR’s panel');
          })
          .then(function (b) {
            b.click();
            return shown('Said A');
          })
          .then(function () { return until(function () { return !!field() && !!at(); }, 5000).catch(function () {}); })
          .then(function () {
            check('the session shown in the PR’s place opens with the bar on the words', !!field() && field().value === 'quokka bridge', field() && field().value);
          });
      })
      .then(function () {
        window.__emit('reveal', { kind: 'session', laneId: A.lane, key: A.key, find: 'quokka bridge' });
        return shown('Said A');
      })
      .then(function () { return until(function () { return !!field(); }, 3000).catch(function () {}); })
      .then(function () {
        type(filter(), 'quokka bridge');
        return until(function () { return !!saidIn(rowOf(A)); }, 5000, 'A found by the search here');
      })
      .then(function () {
        q('.ca-thread-find button[aria-label="Close find"]').click();
        return until(function () { return !bar(); }, 2000);
      })
      .then(function () {
        type(filter(), '');
        return wait(600);
      })
      .then(function () {
        check('a search typed here retires what the quick panel asked', !bar() && title() === 'Said A', title());
      });
  };

  var panel = function () {
    var input = function () { return q('.ca-quick-input textarea'); };
    var row = function () { return all('.ca-quick-row').filter(function (r) { return r.textContent.indexOf('Said A') >= 0; })[0]; };
    return until(function () { return !!A && !!input() && all('.ca-quick-row').length > 0; }, 20000, 'the list')
      .then(function () { return wait(300); })
      .then(function () {
        type(input(), 'quokka bridge');
        return until(function () { var r = row(); return !!r && r.classList.contains('is-active') && !!q('.ca-said', r); }, 5000, 'the session found by what was said');
      })
      .then(function () {
        check('the row shows the line', q('.ca-said', row()).textContent === 'You: ' + LINE, q('.ca-said', row()).textContent);
        key(input(), { key: 'Enter' });
        return wait(300);
      })
      .then(function () {
        var shownMain = sent('show_main').pop();
        var r = shownMain && shownMain.reveal;
        check('⏎ asks the main window to show it with the words', !!r && r.kind === 'session' && r.key === A.key && r.find === 'quokka bridge', JSON.stringify(r));
      });
  };

  say('said search check: waiting for the board');
  until(function () { return !!document.querySelector('.ca-quick-input textarea, .ca-search input'); }, 20000)
    .then(function () { return quick() ? panel() : main(); })
    .then(done)
    .catch(stopped);
})();
