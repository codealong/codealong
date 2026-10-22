// Checks File › Close stale terminals… (ui/CloseStaleDialog.tsx) on a plan of its own: a shell, a
// session and a dev server are listed, each saying what it shows and its terminal, the server
// unticked; the button counts what's ticked; another wait asks the engine again, and an answer
// for an earlier wait that comes in late is let go; the engine is asked to close the ticked tabs;
// a tab that didn't close is listed with why, and the toast counts the ones that did.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/close-stale-check.js
(function () {
  var now = Date.now();
  var tab = function (key, shows, title, ticked, why) {
    return { key: key, terminal: 'Ghostty', shows: shows, title: title, pid: 41, cwd: '/work/app', opened: now - 3 * 864e5, used: now - 2 * 864e5, ticked: ticked, why: why };
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'stale_terminals') {
      window.__calls.push([cmd, args, options]);
      var tabs = [tab('ttys003:41', 'shell', '~/work/app', true, 'unused 2 days · a shell at its prompt'), tab('ttys004:52', 'session', 'Tidy the README', true, '~/work/app · idle 2 days · last reply asks nothing'), tab('ttys005:63', 'program', 'node /usr/local/bin/npm run dev', false, '~/work/app · unused 2 days · closing ends it')];
      var answer = { afterMs: args.afterMs, tabs: args.afterMs > 3 * 864e5 ? [] : tabs, fresh: 4 };
      // Twelve hours answers late, with a fourth tab and every one ticked.
      if (args.afterMs === 432e5) {
        answer.tabs = tabs.concat([tab('ttys006:74', 'shell', '~/work/site', true, 'unused 13 hours · a shell at its prompt')]).map(function (t) { return Object.assign({}, t, { ticked: true }); });
        return new Promise(function (resolve) { setTimeout(function () { resolve(answer); }, 400); });
      }
      return Promise.resolve(answer);
    }
    if (cmd === 'close_stale_terminals') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(
        args.keys.map(function (k) {
          var here = k === 'ttys004:52';
          return { key: k, title: here ? 'Tidy the README' : '', closed: !here, message: here ? 'It’s the tab you’re in.' : 'Closed.' };
        }),
      );
    }
    return real(cmd, args, options);
  };

  var dialog = function () { return q('.ca-idle'); };
  var rows = function () { return all('.ca-idle-list .ca-idle-row'); };
  var ticked = function () { return rows().map(function (r) { return r.querySelector('input').checked ? 'x' : '-'; }).join(''); };
  var button = function () { return all('.ca-dialog-foot button').pop(); };
  var label = function () { var b = button(); return b ? b.textContent : ''; };
  var note = function () { var n = q('.ca-idle-note'); return n ? n.textContent : ''; };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var seg = function (text) { return all('.ca-idle-wait button').find(function (b) { return b.textContent === text; }); };
  // The menu item reaches the window as an event, heard once the window listens for it.
  var open = function () {
    return until(function () {
      window.__emit('close-stale');
      return dialog();
    }, 20000, 'the dialog');
  };

  say('close-stale check: opening');
  open()
    .then(function () { return until(function () { return rows().length === 3; }, 3000, 'three rows'); })
    .then(function () {
      check('it’s called Close stale terminals', /Close stale terminals/.test(q('.ca-dialog-head').textContent), q('.ca-dialog-head').textContent);
      check('the plan is asked for a day', last('stale_terminals').afterMs === 864e5, JSON.stringify(last('stale_terminals')));
      check('each row says what it shows and why', /a shell at its prompt/.test(rows()[0].textContent) && /closing ends it/.test(rows()[2].textContent), rows().map(function (r) { return r.textContent; }).join(' | '));
      check('and its terminal', all('.ca-idle-does').every(function (d) { return d.textContent === 'Ghostty'; }));
      check('the dev server starts unticked', ticked() === 'xx-', ticked());
      check('the button closes the ticked two', label() === 'Close 2 tabs', label());
      check('the tabs used since are counted', note() === '4 other tabs were used more recently.', note());
      rows()[2].querySelector('input').click();
      return wait(50);
    })
    .then(function () {
      check('ticking the server counts it', label() === 'Close 3 tabs' && /3 of 3 ticked/.test(q('.ca-idle-count').textContent), label());
      seg('12 hours').click();
      return wait(50);
    })
    .then(function () {
      seg('1 day').click();
      return wait(800);
    })
    .then(function () {
      check('an earlier wait’s late answer is let go', rows().length === 3 && ticked() === 'xx-' && last('stale_terminals').afterMs === 864e5, rows().length + ' ' + ticked());
      seg('1 week').click();
      return until(function () { return q('.ca-dialog-body .ca-empty') && /unused that long/.test(q('.ca-dialog-body').textContent); }, 3000, 'nothing that long');
    })
    .then(function () {
      check('another wait asks again', last('stale_terminals').afterMs === 7 * 864e5, JSON.stringify(last('stale_terminals')));
      check('with nothing to close, the button is off', button().disabled);
      seg('1 day').click();
      return until(function () { return rows().length === 3; }, 3000, 'three rows again');
    })
    .then(function () {
      check('asked again, the ticks start over', ticked() === 'xx-', ticked());
      button().click();
      return until(function () { return /Not closed/.test(note()); }, 3000, 'what didn’t close');
    })
    .then(function () {
      var asked = last('close_stale_terminals');
      check('the engine is asked to close the ticked tabs', !!asked && asked.keys.join() === 'ttys003:41,ttys004:52' && asked.afterMs === 864e5, JSON.stringify(asked));
      check('the one that stayed says why', rows().length === 1 && /Tidy the README/.test(rows()[0].textContent) && /the tab you’re in/.test(rows()[0].textContent), rows().map(function (r) { return r.textContent; }).join(' | '));
      return until(toast, 3000, 'the toast');
    })
    .then(function () {
      check('the toast counts what closed', /Closed 1 tab\./.test(toast()), toast());
      check('the button is Done', label() === 'Done', label());
    })
    .then(done)
    .catch(stopped);
})();
