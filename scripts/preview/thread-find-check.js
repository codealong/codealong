// Checks finding words in a session's conversation (ThreadFind.tsx): a session read in three
// pages, "lighthouse" said five times in the newest (once in a call a run folds away, once across
// a bold half-word), once in the page before and once in the first. ⌘F in the box opens the bar
// with the keyboard in it; typing marks every match and counts all seven once the pages before
// are read, the run of calls opening to show the one it finds; the latest is the one you're on.
// ⏎ goes back through them and on into the pages before, laying them out from the pages read for
// the count, ⇧⏎ comes forward, ⌘G and ⇧⌘G step from outside the bar, and Escape closes it,
// clears the marks, folds the run again and gives the box its keyboard back. ⌘F opens it again
// with the words ready to type over. A second session says "needle" only two pages back: typing
// it goes there by itself. Words said nowhere say so. The bar takes the place of the prompt pinned
// over the top while it's open. ⌘F finds where you are (findKeys.ts): from the list it goes to the
// filter at the top, after a click on the conversation it finds there, and with the panel taking
// the whole window too; ⌥⌘F goes to the filter from the box, and ⌘K opens the quick panel. In the
// waiting window it works the same from the card's box, and Escape closes only the bar.
//   scripts/preview.sh sessions '' scripts/preview/thread-find-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/thread-find-check.js
//   scripts/preview.sh waiting '' scripts/preview/thread-find-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var long = function (what) {
    var lines = [];
    for (var i = 1; i <= 30; i++) lines.push(what + ', paragraph ' + i + ' of the reply.');
    return lines.join('\n\n');
  };
  var page = function (from, specs, before) {
    var items = specs.map(function (sp, i) {
      return { at: now - (1000 - from - i) * 60e3, kind: sp[0], text: sp[1], failed: false, cursor: from + i, n: 0, id: sp[2], input: sp[3], output: sp[4] };
    });
    return { items: items, before: before, after: from + specs.length };
  };
  var pagesA = {};
  pagesA.newest = page(200, [
    ['you', 'Prompt 4: where is the lighthouse?'],
    ['agent', long('Four') + '\n\nThe **lighthouse** keeper, the Lighthouse beam and the light**house** at night.'],
    ['tool', 'Read · `src/a.ts`', 't1', 'src/a.ts'], ['result', '', 't1', undefined, 'ok'],
    ['tool', 'Edit · `src/lighthouse.ts`', 't2', 'src/lighthouse.ts'], ['result', '', 't2', undefined, 'ok'],
    ['tool', 'Bash · `npm test`', 't3', 'npm test'], ['result', '', 't3', undefined, 'lighthouse tests passed'],
    ['tool', 'Bash · `git status`', 't4', 'git status'], ['result', '', 't4', undefined, 'clean'],
    ['agent', long('Five') + '\n\nAll done.'],
  ], 100);
  pagesA[100] = page(100, [['agent', long('Middle') + '\n\nA lighthouse in the middle.']], 50);
  pagesA[50] = page(0, [['you', 'Prompt 1 about the lighthouse'], ['agent', long('One') + '\n\nNothing here but a needle.']], null);
  var pagesB = {};
  pagesB.newest = page(900, [['you', 'Prompt 9'], ['agent', long('Late B')]], 800);
  pagesB[800] = page(800, [['agent', long('Middle B')]], 700);
  pagesB[700] = page(700, [['you', 'Prompt 7'], ['agent', long('Early B') + '\n\nThe needle is here.']], null);
  var A = null;
  var B = null;
  var hadB = 0;

  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && ((A && args.key === A.key) || (B && args.key === B.key))) {
      window.__calls.push([cmd, args, options]);
      var set = args.key === A.key ? pagesA : pagesB;
      var b = args.before;
      return wait(40).then(function () { return JSON.parse(JSON.stringify(b == null ? set.newest : set[b])); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread' || s.status === 'working') s.status = 'idle';
          if (B || l.hidden || s.visitor || s.archived) return;
          s.status = 'unread';
          s.statusSince = now - (A ? 120e3 : 60e3);
          s.lastActivity = s.statusSince;
          s.hosted = false;
          s.stoppable = false;
          s.title = A ? 'Find B' : 'Find A';
          if (A) B = { key: s.key, lane: l.id };
          else A = { key: s.key, lane: l.id };
        });
      });
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var inWindow = function () { return !!q('.ca-waiting'); };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-sessinsp .ca-composer textarea, .ca-reply .ca-composer textarea'); };
  var thread = function () { return q('.ca-sessinsp .ca-thread, .ca-reply .ca-thread'); };
  var bar = function () { return q('.ca-thread-find'); };
  var filter = function () { return q('.ca-search input'); };
  // The holder of the prompt pinned over the top, which makes way for the bar.
  var pinShown = function () { var p = q('.ca-sessinsp .ca-thread-pin'); return !!p && getComputedStyle(p).display !== 'none'; };
  var field = function () { return q('.ca-thread-find input'); };
  var count = function () { var c = q('.ca-thread-findcount'); return c ? c.textContent : null; };
  var button = function (label) { return q('.ca-thread-find button[aria-label="' + label + '"]'); };
  var press = function (el, init) {
    var e = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init));
    el.dispatchEvent(e);
    return wait(150).then(function () { return e; });
  };
  var marks = function (name) { var h = CSS.highlights.get(name); return h ? Array.from(h) : []; };
  // The match you're on: its words, and whether it's in view below the prompt pinned over the top.
  var at = function () { return marks('ca-find-at')[0] || null; };
  var atText = function () { var r = at(); return r ? r.toString() : null; };
  var inView = function () {
    var r = at();
    if (!r) return false;
    var box = thread().getBoundingClientRect();
    var rect = r.getBoundingClientRect();
    var pin = q('.ca-thread-pinned');
    var top = pin && pin.getClientRects().length > 0 ? pin.getBoundingClientRect().bottom : box.top;
    return rect.height > 0 && rect.top >= top && rect.bottom <= box.bottom;
  };
  var within = function (sel) { var r = at(); return !!r && !!r.startContainer.parentElement.closest(sel); };
  var pagesRead = function (who) { return calls('session_page').filter(function (c) { return c[1].key === who.key; }).map(function (c) { return String(c[1].before); }); };
  var counted = function (text) { return until(function () { return count() === text; }, 4000).catch(function () {}); };
  var open = function (who, name) {
    q('#row-' + CSS.escape(who.lane + '::' + who.key)).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === name && thread() && !/Reading the conversation/.test(thread().textContent); }, 5000).then(function () { return wait(300); });
  };

  // The waiting window, on whichever of the two it opens: every reply says "paragraph" thirty
  // times, four of them in A's pages and three in B's.
  var waiting = function () {
    var shown = null;
    return until(function () { shown = title(); return (shown === 'Find A' || shown === 'Find B') && thread() && box() && !/Reading the conversation/.test(thread().textContent); }, 5000)
      .then(function () { return wait(300); })
      .then(function () { return press(box(), { key: 'f', metaKey: true }); })
      .then(function (e) {
        check('⌘F in the card’s box opens the bar', !!bar() && e.defaultPrevented && document.activeElement === field());
        type(field(), 'paragraph');
        return until(function () { return / of (120|90)$/.test(count() || ''); }, 5000).catch(function () {});
      })
      .then(function () {
        check('it counts the whole conversation', count() === (shown === 'Find A' ? '120 of 120' : '90 of 90'), shown + ': ' + count());
        check('and marks what’s laid out', marks('ca-find').length === (shown === 'Find A' ? 60 : 30) && inView(), String(marks('ca-find').length));
        return press(field(), { key: 'Escape' });
      })
      .then(function (e) {
        check('Escape closes only the bar', e.defaultPrevented && !bar() && inWindow() && title() === shown && document.activeElement === box(), title());
      });
  };

  say('thread find check: waiting for the list');
  until(function () { return B && (inWindow() || q('#row-' + CSS.escape(A.lane + '::' + A.key))); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      if (inWindow()) return waiting().then(function () { throw 'waiting'; });
    })
    .then(function () { return open(A, 'Find A'); })
    .then(function () {
      check('the conversation opens with the keyboard in the box', document.activeElement === box());
      check('with no find bar', !bar());
      return press(box(), { key: 'f', metaKey: true });
    })
    .then(function (e) {
      check('⌘F in the box opens the bar', !!bar() && e.defaultPrevented);
      check('with the keyboard in it', document.activeElement === field(), document.activeElement && document.activeElement.className);
      check('in place of the pinned prompt', !pinShown());
      var el = thread();
      check('and the conversation still at its end', el.scrollHeight - el.scrollTop - el.clientHeight < 2, String(el.scrollHeight - el.scrollTop - el.clientHeight));
      type(field(), 'lighthouse');
      return counted('7 of 7');
    })
    .then(function () {
      check('typing counts every match, the pages before read for it', count() === '7 of 7', count() + ' after pages ' + pagesRead(A).join(','));
      check('the pages before are read once, not laid out', pagesRead(A).join(',') === 'null,100,50' && !/A lighthouse in the middle/.test(thread().textContent), pagesRead(A).join(','));
      check('each match laid out is marked', marks('ca-find').length === 5, String(marks('ca-find').length));
      check('the run of calls opens to show the one it finds', /src\/lighthouse\.ts/.test(thread().textContent));
      check('the latest is the one you’re on, in view', within('.ca-thread-toollabel') && inView(), atText());
      check('nothing later to go to', button('Later match').disabled);
      return press(field(), { key: 'Enter' });
    })
    .then(function () {
      check('⏎ goes to the one before, across a bold half-word', count() === '6 of 7' && atText() === 'lighthouse' && within('.ca-thread-agent') && inView(), count() + ' ' + atText());
      return press(field(), { key: 'Enter' }).then(function () { return press(field(), { key: 'Enter' }); }).then(function () { return press(field(), { key: 'Enter' }); });
    })
    .then(function () {
      check('back to the prompt’s', count() === '3 of 7' && within('.ca-thread-you') && inView(), count() + ' ' + atText());
      return press(field(), { key: 'Enter' }).then(function () { return counted('2 of 7'); });
    })
    .then(function () {
      check('past the first laid out, the page before is laid out and it goes there', count() === '2 of 7' && /A lighthouse in the middle/.test(thread().textContent) && within('.ca-thread-agent') && inView(), count());
      return press(field(), { key: 'Enter' }).then(function () { return counted('1 of 7'); });
    })
    .then(function () {
      check('and on to the first page', count() === '1 of 7' && /Prompt 1 about/.test(at().startContainer.textContent) && inView(), count());
      check('from the pages already read', pagesRead(A).join(',') === 'null,100,50', pagesRead(A).join(','));
      check('nothing earlier to go to', button('Earlier match').disabled);
      return press(field(), { key: 'Enter' });
    })
    .then(function () {
      check('⏎ on the first stays there', count() === '1 of 7', count());
      return press(field(), { key: 'Enter', shiftKey: true });
    })
    .then(function () {
      check('⇧⏎ comes forward', count() === '2 of 7', count());
      field().blur();
      return press(document.body, { key: 'g', metaKey: true });
    })
    .then(function (e) {
      check('⌘G steps back from outside the bar', e.defaultPrevented && count() === '1 of 7', count());
      return press(document.body, { key: 'g', metaKey: true, shiftKey: true });
    })
    .then(function () {
      check('⇧⌘G steps forward', count() === '2 of 7', count());
      field().focus();
      return press(field(), { key: 'Escape' });
    })
    .then(function (e) {
      check('Escape closes the bar', !bar() && e.defaultPrevented);
      check('and clears the marks', marks('ca-find').length === 0 && marks('ca-find-at').length === 0);
      check('the run folds again', !/src\/lighthouse\.ts/.test(thread().textContent));
      check('the box has the keyboard back', document.activeElement === box(), document.activeElement && document.activeElement.className);
      check('the session’s still shown', title() === 'Find A', title());
      check('and the pinned prompt is back', pinShown());
      return press(box(), { key: 'f', metaKey: true });
    })
    .then(function () {
      var f = field();
      check('⌘F opens it again with the words ready to type over', f && f.value === 'lighthouse' && f.selectionStart === 0 && f.selectionEnd === 10 && document.activeElement === f, f && f.value);
      return until(function () { return / of 7$/.test(count() || ''); }, 4000).catch(function () {});
    })
    .then(function () {
      // On the one nearest where you were reading, which went back to the second.
      check('marked again, all laid out now, the one nearest in view', / of 7$/.test(count()) && marks('ca-find').length === 7 && inView(), count() + ' ' + marks('ca-find').length);
      type(field(), 'zqxj');
      return counted('No matches');
    })
    .then(function () {
      check('words said nowhere say so', count() === 'No matches' && marks('ca-find').length === 0, count());
      return press(field(), { key: 'Escape' });
    })
    .then(function () {
      // Only what this panel reads: the list may have read B's conversation before.
      hadB = pagesRead(B).length;
      return open(B, 'Find B');
    })
    .then(function () {
      check('another session opens with no bar', !bar());
      return press(box(), { key: 'f', metaKey: true });
    })
    .then(function () {
      check('its bar starts empty', field() && field().value === '', field() && field().value);
      type(field(), 'needle');
      return until(function () { return count() === '1 of 1' && at(); }, 5000).catch(function () {});
    })
    .then(function () {
      check('words said only further back go there by themselves', count() === '1 of 1' && /The needle is here/.test(at().startContainer.textContent) && inView(), count() + ' ' + atText());
      check('reading back the pages between', pagesRead(B).slice(hadB).join(',') === 'null,800,700', pagesRead(B).join(','));
      check('the keyboard stays in the bar', document.activeElement === field());
      return press(field(), { key: 'Escape' });
    })
    // ⌘F finds where you are (findKeys.ts): from the list, the filter at the top.
    .then(function () {
      var list = q('.ca-board-list');
      list.focus();
      return press(list, { key: 'f', metaKey: true });
    })
    .then(function (e) {
      check('⌘F from the list goes to the filter', e.defaultPrevented && document.activeElement === filter() && !bar(), document.activeElement && document.activeElement.className);
      return press(filter(), { key: 'Escape' });
    })
    .then(function () {
      // A click on the conversation's text leaves the keyboard with nothing.
      q('.ca-sessinsp .ca-thread-agent').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      return press(document.body, { key: 'f', metaKey: true });
    })
    .then(function (e) {
      check('after a click on the conversation it finds there', e.defaultPrevented && !!bar() && document.activeElement === field(), document.activeElement && document.activeElement.className);
      return press(field(), { key: 'Escape' });
    })
    .then(function () {
      box().focus();
      return press(box(), { key: 'ƒ', code: 'KeyF', metaKey: true, altKey: true });
    })
    .then(function (e) {
      check('⌥⌘F goes to the filter from the box', e.defaultPrevented && document.activeElement === filter() && !bar(), document.activeElement && document.activeElement.className);
      return press(filter(), { key: 'Escape' });
    })
    .then(function () {
      var before = calls('show_quick').length;
      return press(document.body, { key: 'k', metaKey: true }).then(function (e) {
        check('⌘K opens the quick panel', e.defaultPrevented && calls('show_quick').length === before + 1 && document.activeElement !== filter());
      });
    })
    .then(function () {
      // The filter had the keyboard last, but with the panel taking the whole window there's no
      // list for it to filter.
      return press(document.body, { key: 'f', metaKey: true, shiftKey: true });
    })
    .then(function () {
      check('the panel takes the whole window', !!q('.ca-win-body.is-panel-max'));
      return press(document.body, { key: 'f', metaKey: true });
    })
    .then(function (e) {
      check('where ⌘F finds in the conversation', e.defaultPrevented && !!bar() && document.activeElement === field(), document.activeElement && document.activeElement.className);
      return press(field(), { key: 'Escape' }).then(function () { return press(document.body, { key: 'f', metaKey: true, shiftKey: true }); });
    })
    .then(done)
    .catch(function (e) {
      if (e === 'waiting') done();
      else stopped(e);
    });
})();
