// Checks the strip over a session's panel (SessionPanel.tsx `QueueStrip`, `fitFor`) from wide to
// narrow: its buttons keep to one line and inside it, and the actions it hasn't room for are in
// its ⋯ menu instead, the last first, ⋯ there only then, and none of them there that would have
// fit. ⋯ drops a menu of just those, each with its key, Snooze with its times, and picking one
// does what its button does. What it says keeps room for a word and its ellipsis throughout.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/strip-fold-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/strip-fold-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');

  var strip = function () { return q('.ca-sessinsp .ca-reply-queue.has-keys'); };
  var shows = function (el) { return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'; };
  var label = function (b) { return (q(':scope > span', b) || {}).textContent || ''; };
  var wide = function (el) { return el.getBoundingClientRect().width; };

  // How the strip stands at its width: what's wrong with it, or null.
  var standing = function (s) {
    var actions = all('.ca-hotkey.is-foldable', s);
    var more = q('.ca-hotkey.is-more', s);
    var folded = actions.filter(function (b) { return b.classList.contains('is-folded'); });
    var shown = all('.ca-hotkeys > *', s).filter(shows);
    var tops = shown.map(function (b) { var r = b.getBoundingClientRect(); return Math.round(r.top + r.height / 2); });
    var end = s.getBoundingClientRect().right - parseFloat(getComputedStyle(s).paddingRight);
    var past = Math.max.apply(null, shown.map(function (b) { return b.getBoundingClientRect().right; })) - end;
    var noteEl = q('.ca-reply-queue-note', s);
    var spare = wide(noteEl) - parseFloat(getComputedStyle(noteEl).minWidth);
    var gap = parseFloat(getComputedStyle(q('.ca-hotkeys', s)).columnGap) || 0;
    var first = actions.indexOf(folded[0]);
    if (tops.some(function (t) { return Math.abs(t - tops[0]) > 1; })) return 'the buttons took two lines';
    if (folded.length && folded.some(shows)) return 'one in ⋯ shows on the strip too';
    if (folded.length && actions.slice(first).some(function (b) { return !b.classList.contains('is-folded'); })) return 'what’s in ⋯ isn’t the last of them';
    if (shows(more) !== folded.length > 0) return folded.length ? 'no ⋯ for ' + folded.map(label).join(', ') : 'a ⋯ with nothing in it';
    if (past > 0.5 && folded.length < actions.length) return 'the buttons ran ' + past.toFixed(1) + 'px past the strip';
    if (spare < -0.5) return 'what it says has ' + spare.toFixed(1) + 'px less than it keeps';
    // The first in ⋯ would have fit beside the rest, ⋯ going too where it's the only one.
    if (folded.length) {
      var back = wide(folded[0]) + gap - (folded.length === 1 ? wide(more) + gap : 0);
      var roomy = q('.ca-hotkey.is-roomy', s);
      if (!roomy && back <= spare - 0.5) return label(folded[0]) + ' is in ⋯ with ' + spare.toFixed(1) + 'px spare, and needs ' + back.toFixed(1);
    }
    return null;
  };

  say('strip fold check: waiting for a session’s panel');
  until(function () { return strip() && q('.ca-hotkey.is-more', strip()); }, 20000, 'no strip with ⋯ over a session’s panel')
    .then(function () { return wait(300); })
    .then(async function () {
      var s = strip();
      var widths = [1100, 900, 760, 640, 560, 500, 460, 420, 380, 340, 300, 260];
      var wrong = null;
      var most = 0;
      var none = false;
      for (var i = 0; i < widths.length && !wrong; i++) {
        s.style.width = widths[i] + 'px';
        await wait(120);
        wrong = standing(s);
        var n = all('.ca-hotkey.is-folded.is-foldable', s).length;
        most = Math.max(most, n);
        if (!n) none = true;
        if (wrong) check('at ' + widths[i] + 'px the strip is one line, what it hasn’t room for in ⋯', false, wrong);
      }
      if (!wrong) check('from ' + widths[0] + ' to ' + widths[widths.length - 1] + 'px the strip is one line, what it hasn’t room for in ⋯', none && most >= 2, none ? 'at most ' + most + ' in ⋯' : 'something was always in ⋯');

      // Narrow enough for some to go into ⋯, and its menu has just those.
      s.style.width = '340px';
      await wait(120);
      var folded = all('.ca-hotkey.is-folded.is-foldable', s);
      var more = q('.ca-hotkey.is-more', s);
      check('⋯ says what’s in it', more.title === 'More: ' + folded.map(label).join(', '), more.title);
      var before = menus();
      more.click();
      await until(function () { return menus() > before; }, 3000, 'no menu from ⋯');
      var items = lastMenu();
      var tops = items.map(function (it) { return it.text; });
      check('⋯ drops a menu of what’s in it, in their order', tops.join('|') === folded.map(label).join('|'), tops.join('|') + ' for ' + folded.map(label).join('|'));
      var chords = (window.__CODEALONG_CHORDS__ || {}).session || {};
      var keyed = { Seen: chords.seen, Done: chords.done, Close: chords.close, 'End job': chords.close, Archive: chords.archive, Unarchive: chords.archive };
      var unkeyed = items.filter(function (it) { return keyed[it.text] && !it.accelerator; });
      check('each with its key', unkeyed.length === 0, unkeyed.map(function (it) { return it.text; }).join(', '));
      var snooze = items.filter(function (it) { return it.text === 'Snooze'; })[0];
      if (snooze) check('Snooze with its times', !!snooze.items && snooze.items.length >= 4, JSON.stringify(snooze.items && snooze.items.map(function (it) { return it.text; })));
      var archive = items.filter(function (it) { return /^(Un)?archive$/i.test(it.text); })[0];
      if (archive && archive.enabled !== false) {
        var asked = calls('archive_session').length;
        pick(archive);
        await until(function () { return calls('archive_session').length > asked; }, 3000, 'picking ' + archive.text + ' archived nothing').catch(function () {});
        check('picking ' + archive.text + ' does what its button does', calls('archive_session').length > asked, JSON.stringify(calls('archive_session')));
      }
      s.style.width = '';
    })
    .then(done, stopped);
})();
