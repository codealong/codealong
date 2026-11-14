// Checks the window's find (ui/WindowFind.tsx) over the Schedules view, the sample's first
// schedule selected, and some lines the check adds to the page. ⌘F opens a bar with the keyboard
// in it; typing marks every match the window shows, in the list and the panel beside it alike, and
// counts them, on the first in view where you are; it narrows nothing. It finds words across bold
// within a line, but not from one line into the next, and passes over what doesn't show (hidden
// by display, visibility or opacity, or cut off past an ellipsis or in a scroller cut off itself),
// what's marked aria-hidden or data-find-skip, and what a field holds; words split across inline
// elements are boxed. A line added while it's open is found as it comes, the match you're on staying. ⏎ steps
// on, round from the last to the first, ⇧⏎ back; ⌘G and ⇧⌘G step from outside the bar. Past 1000
// matches it marks 1000 and says 1000+. Escape closes it, clears the marks and gives the keyboard
// back. ⌥⌘F and / open the list's own find instead, and in Settings ⌘F is Settings' search.
//   PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules '' scripts/preview/window-find-check.js
//   PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json PREVIEW_PLATFORM=linux scripts/preview.sh schedules '' scripts/preview/window-find-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'schedules');
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || window.__CODEALONG_PLATFORM__ === 'windows' || /\bLinux\b/.test(navigator.userAgent);
  var cmdKey = function (init) { return Object.assign({ metaKey: !linux, ctrlKey: linux }, init); };
  var bar = function () { return q('.ca-findbar'); };
  var field = function () { return q('.ca-findbar input'); };
  var count = function () { var c = q('.ca-findbar-count'); return c ? c.textContent : null; };
  var filter = function () { return q('.ca-listfind input'); };
  var press = function (el, init) {
    var e = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init));
    el.dispatchEvent(e);
    return wait(150).then(function () { return e; });
  };
  var marks = function (name) { var h = CSS.highlights.get(name); return h ? Array.from(h) : []; };
  // How many matches are laid out, which the marks' layer says; and whether a box marks one in an
  // element `sel` matches (the one you're on is a highlight, the rest boxes).
  var shown = function () { var l = q('.ca-findmarks'); return l ? Number(l.dataset.shown) : 0; };
  var boxes = function () { return all('.ca-findmarks > span'); };
  var boxIn = function (sel) {
    return boxes().some(function (b) {
      var r = b.getBoundingClientRect();
      var x = r.left + r.width / 2;
      var y = r.top + r.height / 2;
      return all(sel).some(function (el) { var e = el.getBoundingClientRect(); return x >= e.left && x <= e.right && y >= e.top && y <= e.bottom; });
    });
  };
  var at = function () { return marks('ca-find-at')[0] || null; };
  var atIn = function (sel) { var r = at(); return !!r && !!r.startContainer.parentElement.closest(sel); };
  var counted = function (re) { return until(function () { return re.test(count() || ''); }, 4000).catch(function () {}); };
  var fixture = document.createElement('div');
  fixture.id = 'wf-fixture';
  fixture.setAttribute('style', 'position: fixed; left: 300px; bottom: 0; width: 400px; max-height: 200px; overflow: auto; background: white; font-size: 10px;');
  // Laid out tight, so its first lines show without scrolling: the check's page draws no frames,
  // which the boxes are drawn again in once something scrolls.
  fixture.innerHTML = [
    '<style>#wf-fixture p { margin: 0; line-height: 12px; }</style>',
    '<p id="wf-one">zqvx one</p>',
    '<p style="display: none">zqvx hidden</p>',
    '<p style="visibility: hidden">zqvx invisible</p>',
    '<p style="opacity: 0">zqvx faded</p>',
    '<p aria-hidden="true">zqvx aria</p>',
    '<p data-find-skip>zqvx skipped</p>',
    '<input value="zqvx in a field">',
    '<textarea>zqvx in a box</textarea>',
    '<p id="wf-bold">zq<b>vx</b> across bold</p>',
    '<div>zq</div><div>vx</div>',
    '<p style="width: 60px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis">a long line cut off before zqvx</p>',
  ].join('');
  var name = null;
  var back = null;

  say('window find check: waiting for the schedules');
  until(function () { return all('.ca-schedrow').length > 1; }, 20000, 'the schedules')
    .then(function () {
      var row = all('.ca-schedrow')[0];
      name = q('.ca-schedrow-name', row).textContent;
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return until(function () { return q('.ca-schedinsp') && q('.ca-schedinsp').textContent.indexOf(name) >= 0; }, 3000, 'the schedule’s panel');
    })
    .then(function () {
      back = document.activeElement;
      return press(document.body, cmdKey({ key: 'f', code: 'KeyF' }));
    })
    .then(function (e) {
      check('⌘F opens the bar with the keyboard in it', e.defaultPrevented && !!bar() && document.activeElement === field(), document.activeElement && document.activeElement.className);
      check('not the list’s find', !filter());
      var rows = all('.ca-schedrow').length;
      type(field(), name);
      return counted(/ of \d+$/).then(function () { return rows; });
    })
    .then(function (rows) {
      check('it marks the words in the list and the panel alike', (atIn('.ca-schedrow') || boxIn('.ca-schedrow')) && (atIn('.ca-schedinsp') || boxIn('.ca-schedinsp')), shown() + ' found, ' + boxes().length + ' boxes');
      check('and counts what it marks', shown() >= 2 && new RegExp(' of ' + shown() + '$').test(count()), count() + ' / ' + shown());
      check('it narrows nothing', all('.ca-schedrow').length === rows, all('.ca-schedrow').length + ' rows, ' + rows + ' before');
      check('starting in the list, where you are', atIn('.ca-board'), at() && at().startContainer.parentElement.className);
      document.body.appendChild(fixture);
      type(field(), 'zqvx');
      return counted(/ of 2$/);
    })
    .then(function () {
      check('only what shows is found, across bold within a line but not from one line into the next', count() !== null && / of 2$/.test(count()) && shown() === 2, count() + ' / ' + shown());
      if (!atIn('#wf-bold')) field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      check('the bold one is one match', at() && at().toString() === 'zqvx' && at().startContainer.parentElement.id === 'wf-bold' && at().endContainer.parentElement.tagName === 'B', at() && at().toString());
      check('the other is boxed, the one you’re on drawn in ink', boxIn('#wf-one') && boxes().length === 1 && !boxIn('#wf-bold'), boxes().length + ' boxes');
      var p = document.createElement('p');
      p.id = 'wf-later';
      p.textContent = 'zqvx later';
      fixture.appendChild(p);
      var was = at() && at().toString() + at().startContainer.parentElement.id;
      return counted(/ of 3$/).then(function () { return was; });
    })
    .then(function (was) {
      check('a line added while it’s open is found as it comes', / of 3$/.test(count() || '') && shown() === 3, count());
      check('the match you’re on stays', !!at() && at().toString() + at().startContainer.parentElement.id === was, was);
      return press(field(), { key: 'Enter' }).then(function () { return count(); });
    })
    .then(function (first) {
      var n = parseInt(first, 10);
      return press(field(), { key: 'Enter' }).then(function () { return press(field(), { key: 'Enter' }); }).then(function () {
        check('⏎ steps on, round from the last to the first', parseInt(count(), 10) === ((n + 1) % 3) + 1, first + ' then ' + count());
        return press(field(), { key: 'Enter', shiftKey: true });
      }).then(function () {
        check('⇧⏎ steps back', parseInt(count(), 10) === ((n + 0) % 3) + 1, count());
        field().blur();
        return press(document.body, cmdKey({ key: 'g', code: 'KeyG' }));
      }).then(function (e) {
        check('⌘G steps on from outside the bar', e.defaultPrevented && parseInt(count(), 10) === ((n + 1) % 3) + 1, count());
        return press(document.body, cmdKey({ key: 'g', code: 'KeyG', shiftKey: true }));
      }).then(function () {
        check('⇧⌘G steps back', parseInt(count(), 10) === ((n + 0) % 3) + 1, count());
      });
    })
    .then(function () {
      // Words split across inline elements, and a box that scrolls but is cut off itself where
      // nothing scrolls it into sight.
      var more = document.createElement('div');
      more.innerHTML = [
        '<p id="wf-split"><b>qx</b><em>split</em> one</p>',
        '<p id="wf-split2">qxsplit two</p>',
        '<div style="height: 12px; overflow: hidden"><div style="height: 14px; overflow: auto; margin-top: 30px">a<br>b<br>qxsplit nested</div></div>',
      ].join('');
      fixture.insertBefore(more, fixture.firstChild);
      field().focus();
      type(field(), 'qxsplit');
      return counted(/ of 2$/);
    })
    .then(function () {
      check('what a box that’s cut off itself scrolls isn’t found', / of 2$/.test(count() || '') && shown() === 2, count());
      if (!atIn('#wf-split2')) field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      check('words split across inline elements are boxed', atIn('#wf-split2') && boxIn('#wf-split'), boxes().length + ' boxes');
    })
    .then(function () {
      var many = document.createElement('p');
      for (var i = 0; i < 1100; i++) many.appendChild(document.createElement('span')).textContent = 'qzcap ';
      fixture.insertBefore(many, fixture.firstChild);
      field().focus();
      type(field(), 'qzcap');
      return counted(/1000\+$/);
    })
    .then(function () {
      check('past 1000 matches it marks 1000 and says so', /of 1000\+$/.test(count() || '') && shown() === 1000, count() + ' / ' + shown());
      check('with boxes only for those in sight', boxes().length > 0 && boxes().length < 300, boxes().length + ' boxes');
      return press(field(), { key: 'Escape' });
    })
    .then(function (e) {
      check('Escape closes it', e.defaultPrevented && !bar());
      check('and clears the marks', shown() === 0 && boxes().length === 0 && marks('ca-find-at').length === 0);
      check('giving the keyboard back', document.activeElement === back, document.activeElement && document.activeElement.className);
      fixture.remove();
      return press(document.body, cmdKey({ key: 'ƒ', code: 'KeyF', altKey: true }));
    })
    .then(function (e) {
      check('⌥⌘F opens the list’s find instead', e.defaultPrevented && !!filter() && document.activeElement === filter() && !bar(), document.activeElement && document.activeElement.className);
      return press(filter(), { key: 'Escape' });
    })
    .then(function () {
      q('.ca-board-list, .ca-board').focus();
      return press(document.activeElement || document.body, { key: '/', code: 'Slash' });
    })
    .then(function () {
      check('and so does /', !!filter() && !bar());
      return press(filter(), { key: 'Escape' });
    })
    .then(function () {
      return press(document.body, cmdKey({ key: ',', code: 'Comma' }));
    })
    .then(function () {
      return until(function () { return q('.ca-settings-find input'); }, 3000, 'Settings');
    })
    .then(function () {
      q('.ca-settings-find input').blur();
      return press(document.body, cmdKey({ key: 'f', code: 'KeyF' }));
    })
    .then(function () {
      check('in Settings ⌘F is its own search', !bar() && document.activeElement === q('.ca-settings-find input'), document.activeElement && document.activeElement.className);
    })
    .then(done)
    .catch(stopped);
})();
