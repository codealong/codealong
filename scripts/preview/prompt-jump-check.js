// Checks the keys that jump between your prompts in a session's conversation (SessionThread.tsx
// `jump`): a session read in three pages, the newest with prompts 4 to 6, one before it with no
// prompt at all, and the first with prompts 1 to 3. ⌘⇧↑ in the box brings up the latest prompt,
// then the one before; ⌘↑ in the box is the box's; from the list ⌘↑ and ⌘↓ jump and the list's
// selection stays; up past what's read reads on through the page with no prompt to the one before
// it; ⌘↓ past the last prompt goes to the end. The waiting window opens on it, and the keys work
// from its box the same way. A second session whose newest page has no prompt, nor the one before
// it, pins the prompt two pages back from the start, and a click on it reads up to it with the
// pages already read.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/prompt-jump-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/prompt-jump-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var picked = null;
  var long = function (what) {
    var lines = [];
    for (var i = 1; i <= 30; i++) lines.push(what + ', paragraph ' + i + ' of the reply.');
    return lines.join('\n\n');
  };
  var pages = {};
  var page = function (from, specs, before) {
    var items = specs.map(function (sp, i) {
      return { at: now - (1000 - from - i) * 60e3, kind: sp[0], text: sp[1], failed: false, cursor: from + i, n: 0, id: sp[2], input: sp[3], output: sp[4] };
    });
    return { items: items, before: before, after: from + specs.length };
  };
  pages.newest = page(200, [['you', 'Prompt 4'], ['agent', long('Four')], ['you', 'Prompt 5'], ['agent', long('Five')], ['you', 'Prompt 6'], ['agent', long('Six')]], 100);
  var tools = [];
  for (var t = 0; t < 8; t++) tools.push(['tool', 'Bash · `step ' + t + '`', 't' + t, 'step ' + t], ['result', '', 't' + t, undefined, 'ok']);
  tools.push(['agent', long('Middle')]);
  pages[100] = page(100, tools, 50);
  pages[50] = page(0, [['you', 'Prompt 1'], ['agent', long('One')], ['you', 'Prompt 2'], ['agent', long('Two')], ['you', 'Prompt 3'], ['agent', long('Three')]], null);
  var pagesB = {};
  pagesB.newest = page(900, [['tool', 'Bash · `late`', 'b1', 'late'], ['result', '', 'b1', undefined, 'ok'], ['agent', long('Late B')]], 800);
  pagesB[800] = page(800, [['agent', long('Middle B')]], 700);
  pagesB[700] = page(700, [['you', 'Prompt 9'], ['agent', long('Nine')]], null);
  var pickedB = null;

  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && ((picked && args.key === picked.key) || (pickedB && args.key === pickedB.key))) {
      window.__calls.push([cmd, args, options]);
      var set = args.key === picked.key ? pages : pagesB;
      var b = args.before;
      return wait(40).then(function () { return JSON.parse(JSON.stringify(b == null ? set.newest : set[b])); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread' || s.status === 'working') s.status = 'idle';
          if (pickedB || l.hidden || s.visitor || s.archived) return;
          s.status = 'unread';
          // A's turn ended last, so the waiting window opens on it first.
          s.statusSince = now - (picked ? 120e3 : 60e3);
          s.lastActivity = s.statusSince;
          s.hosted = false;
          s.stoppable = false;
          s.title = picked ? 'Jump B' : 'Jump A';
          if (picked) pickedB = { key: s.key, lane: l.id };
          else picked = { key: s.key, lane: l.id };
        });
      });
      // No PR waits on your review either: a review would come before A in the waiting window.
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var inWindow = function () { return !!document.querySelector('.ca-waiting'); };
  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea, .ca-reply .ca-composer textarea'); };
  var thread = function () { return document.querySelector('.ca-sessinsp .ca-thread, .ca-reply .ca-thread'); };
  var row = function () { return document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var key = function (el, init) {
    var e = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init));
    el.dispatchEvent(e);
    return e;
  };
  var pages_read = function (who) { return window.__calls.filter(function (c) { return c[0] === 'session_page' && c[1].key === (who || picked).key; }).map(function (c) { return String(c[1].before); }); };
  // The prompt at the top of the view, if one sits there. The padding is in the view's own pixels,
  // which a card drawn at a zoom makes bigger on the screen (zoom.ts `scaleOf`).
  var atTop = function () {
    var el = thread();
    var pad = parseFloat(getComputedStyle(el).paddingTop) * (el.getBoundingClientRect().width / el.offsetWidth);
    var top = el.getBoundingClientRect().top + pad;
    var found = null;
    el.querySelectorAll('[data-prompt]').forEach(function (p) {
      if (Math.abs(p.getBoundingClientRect().top - top) < 3) found = p.querySelector('.ca-thread-you').textContent.trim();
    });
    return found;
  };
  var atEnd = function () { var el = thread(); return el.scrollHeight - el.scrollTop - el.clientHeight < 2; };
  var press = function (init) {
    var e = key(document.activeElement || document.body, Object.assign({ key: 'ArrowUp', metaKey: true }, init));
    return wait(200).then(function () { return e; });
  };

  var opened = function () {
    return until(function () { return title() === 'Jump A' && thread() && /Prompt 6/.test(thread().textContent); }, 5000).then(function () { return wait(300); });
  };

  // At the end, the latest prompt has gone off the top and is pinned there; a click on it brings
  // it up, and it's no longer pinned while it's in view.
  var pinned = function () { var p = document.querySelector('.ca-thread-pinned'); return p ? p.textContent.trim() : null; };
  var pinChecks = function () {
    var p = document.querySelector('.ca-thread-pinned');
    check('at the end the latest prompt is pinned', pinned() === 'Prompt 6', pinned());
    check('flush with the top of the view', p && Math.abs(p.getBoundingClientRect().top - thread().getBoundingClientRect().top) < 1, p && String(p.getBoundingClientRect().top - thread().getBoundingClientRect().top));
    if (!p) return Promise.resolve();
    p.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    p.click();
    return wait(200).then(function () {
      check('a click brings it up', atTop() === 'Prompt 6', atTop());
      check('and it’s no longer pinned', pinned() === null, pinned());
      thread().scrollTop = thread().scrollHeight;
      return wait(200);
    });
  };

  // A session whose first two pages have no prompt: the one before them is pinned from the start,
  // and a click reads up to it with the pages already read.
  var readBehind = function () {
    // Only what this panel reads: the list may have shown B before.
    var had = pages_read(pickedB).length;
    var since = function () { return pages_read(pickedB).slice(had).join(','); };
    document.getElementById('row-' + pickedB.lane + '::' + pickedB.key).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === 'Jump B' && pinned() === 'Prompt 9'; }, 5000)
      .catch(function () {})
      .then(function () {
        check('with no prompt in the first pages, the one before them is pinned', pinned() === 'Prompt 9', pinned() + ' after pages ' + since());
        check('read back once, page by page', since() === 'null,800,700', since());
        var p = document.querySelector('.ca-thread-pinned');
        if (p) p.click();
        return until(function () { return atTop() === 'Prompt 9'; }, 3000).catch(function () {});
      })
      .then(function () {
        check('a click on it reads up to it', atTop() === 'Prompt 9', atTop());
        check('from the pages already read', since() === 'null,800,700', since());
      });
  };

  // The waiting window: its box, both ways, and on to the end.
  var waiting = function () {
    return opened()
      .then(function () {
        check('it opens at the end', title() === 'Jump A' && atEnd(), title());
        check('with the cursor in its box', document.activeElement === box());
        return press({ shiftKey: true });
      })
      .then(function () {
        check('⌘⇧↑ brings up the latest prompt', atTop() === 'Prompt 6', atTop());
        return press({ shiftKey: true });
      })
      .then(function () {
        check('again, the one before', atTop() === 'Prompt 5', atTop());
        return press({});
      })
      .then(function (e) {
        check('⌘↑ is the box’s', !e.defaultPrevented && atTop() === 'Prompt 5', atTop());
        return press({ key: 'ArrowDown', shiftKey: true });
      })
      .then(function () {
        check('⌘⇧↓ goes to the next', atTop() === 'Prompt 6', atTop());
        return press({ key: 'ArrowDown', shiftKey: true });
      })
      .then(function () {
        check('and past it, the end', atEnd());
        return pinChecks();
      });
  };

  say('prompt jump check: waiting for the list');
  until(function () { return pickedB && (row() || inWindow()); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      if (inWindow()) return waiting().then(function () { throw 'waiting'; });
    })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return opened();
    })
    .then(function () {
      check('the conversation opens at the end', atEnd());
      check('the cursor is in the box', document.activeElement === box());
      return press({ shiftKey: true });
    })
    .then(function (e) {
      check('⌘⇧↑ in the box brings up the latest prompt', atTop() === 'Prompt 6', atTop());
      check('and the box doesn’t select', e.defaultPrevented && document.activeElement === box());
      return press({ shiftKey: true });
    })
    .then(function () {
      check('again, the one before', atTop() === 'Prompt 5', atTop());
      return press({});
    })
    .then(function (e) {
      check('⌘↑ in the box is the box’s', !e.defaultPrevented && atTop() === 'Prompt 5', atTop());
      key(box(), { key: 'Escape' });
      return wait(100);
    })
    .then(function () {
      check('Escape gives the list the keys', document.activeElement && !!document.activeElement.closest('.ca-board-list'), document.activeElement && document.activeElement.className);
      return press({});
    })
    .then(function () {
      check('⌘↑ from the list jumps', atTop() === 'Prompt 4', atTop());
      check('and the list’s selection stays', title() === 'Jump A', title());
      return press({ key: 'ArrowDown' });
    })
    .then(function () {
      check('⌘↓ goes to the next', atTop() === 'Prompt 5', atTop());
      return press({ key: 'ArrowUp' }).then(function () { return wait(300); });
    })
    .then(function () {
      check('back at the first prompt read', atTop() === 'Prompt 4', atTop());
      return press({ shiftKey: true }).then(function () {
        return until(function () { return atTop() === 'Prompt 3'; }, 3000).catch(function () {});
      });
    })
    .then(function () {
      check('up past what’s read reads on through a page with no prompt', atTop() === 'Prompt 3' && pages_read().join(',') === 'null,100,50', atTop() + ' after pages ' + pages_read().join(','));
      return press({ shiftKey: true }).then(function () { return press({ shiftKey: true }); });
    })
    .then(function () {
      check('on up to the first prompt', atTop() === 'Prompt 1', atTop());
      return press({ shiftKey: true });
    })
    .then(function () {
      check('past the first, the start of the session', thread().scrollTop === 0 && /The start of the session/.test(thread().textContent), String(thread().scrollTop));
      return press({ key: 'ArrowDown', shiftKey: true });
    })
    .then(function () {
      check('⌘⇧↓ from the start goes to the first prompt', atTop() === 'Prompt 1', atTop());
      var go = Promise.resolve();
      for (var i = 0; i < 5; i++) go = go.then(function () { return press({ key: 'ArrowDown' }); });
      return go;
    })
    .then(function () {
      check('down to the latest prompt', atTop() === 'Prompt 6', atTop());
      return press({ key: 'ArrowDown' });
    })
    .then(function () {
      check('and past it, the end', atEnd());
      var landed = document.querySelectorAll('.ca-thread-said.is-landed');
      check('prompts it brought up are marked', landed.length > 0, String(landed.length));
      return pinChecks();
    })
    .then(function () { return readBehind(); })
    .then(function () {
      // With Settings open (or the rename sheet) the keys are theirs.
      key(document.body, { key: ',', metaKey: true });
      return until(function () { return document.querySelector('.ca-settings'); }, 3000).then(function () {
        var before = thread().scrollTop;
        return press({}).then(function () {
          check('Settings keeps the keys', thread().scrollTop === before, String(thread().scrollTop - before));
        });
      });
    })
    .catch(function (e) { if (e !== 'waiting') check('ran', false, String(e) + ' picked ' + !!picked + ' ' + !!pickedB + ' waiting ' + inWindow()); })
    .then(done);
})();
