// Checks a search in the quick panel finding a session on another machine (quick.ts
// `findSessions` with `Machines`): found by its title and by what was said in it there (the
// machine's own text search, `search_devices`), its row carries the machine's badge, ⏎ and ⌥⏎
// both show it in Codealong (it opens where it lives only on that machine), and ⇥ answers it in
// Waiting on you while that lists the machine's sessions.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh quick '' scripts/preview/quick-devices-check.js
(function () {
  // This runs before the app's script: a session on the Mac mini gets a title with a word
  // nothing else has, and another there is found only by what was said in it.
  var WORD = 'q' + Math.random().toString(36).slice(2, 10) + 'xz';
  var SAID = 'q' + Math.random().toString(36).slice(2, 10) + 'zx';
  var picked = null;
  var spoken = null;
  // What the machines' text search was asked, answered here before the stub records it.
  var devicesAsked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The stub finds two sessions by what was said in them for any words; nothing here does.
    if (cmd === 'search_text') return Promise.resolve({ hits: [] });
    if (cmd === 'search_devices') {
      var asked = (args && args.words) || [];
      devicesAsked.push(asked.join(' '));
      var hits = spoken && asked.indexOf(SAID) >= 0 ? [{ key: spoken.key, snippet: 'we left the ' + SAID + ' for later', you: true, at: Date.now() - 60000, score: 7 }] : [];
      return Promise.resolve([{ device: '9f8e7d6c5b', name: 'Mac mini', found: { hits: hits, building: false }, error: null }]);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'devices') return p;
    return p.then(function (view) {
      var there = view.lanes.filter(function (l) { return l.device && l.device.online && l.sessions.some(function (s) { return !s.visitor && !s.archived; }); });
      there.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.visitor || s.archived) return;
          if (!picked || picked.key === s.key) {
            s.title = 'Look into the ' + WORD + ' notes';
            picked = { laneId: l.id, key: s.key, device: l.device.name };
          } else if (!spoken || spoken.key === s.key) {
            spoken = { laneId: l.id, key: s.key };
          }
        });
      });
      return view;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row'); };
  var rowOf = function (word) { return rows().filter(function (r) { return r.textContent.indexOf(word) >= 0; })[0]; };
  var clear = function () { type(input(), ''); return wait(200); };

  say('quick devices check: waiting for the list');
  until(function () { return !!input() && rows().length > 0 && calls('devices').length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      if (!picked) {
        check('a session on a connected machine', false, 'devices-sample.json lent the mini no sessions');
        return;
      }
      type(input(), WORD);
      return until(function () { return !!rowOf(WORD); }, 5000)
        .catch(function () {})
        .then(function () {
          var row = rowOf(WORD);
          check('a session on another machine is found by its title', !!row, rows().map(function (r) { return r.textContent.slice(0, 40); }).join(' | '));
          if (!row) return;
          var badge = row.querySelector('.ca-devbadge');
          check('its row says the machine', !!badge && badge.textContent.indexOf('Mac mini') >= 0, badge ? badge.textContent : row.textContent);
          check('its tooltip says the machine and leaves out ⌥⏎', row.title.indexOf('on ' + picked.device) >= 0 && row.title.indexOf('⌥⏎') < 0, row.title);
          key(input(), { key: 'Enter' });
          return wait(300).then(function () {
            var shown = sent('show_main').pop();
            check('⏎ shows it in Codealong', !!shown && !!shown.reveal && shown.reveal.key === picked.key && shown.reveal.laneId === picked.laneId, JSON.stringify(shown));
          });
        });
    })
    .then(function () {
      if (!picked) return;
      // ⌥⏎ would open it where it lives, which is on the mini: Codealong shows it instead.
      type(input(), WORD);
      return until(function () { return !!rowOf(WORD); }, 5000)
        .catch(function () {})
        .then(function () {
          var before = sent('show_main').length;
          key(input(), { key: 'Enter', altKey: true });
          return wait(300).then(function () {
            var shown = sent('show_main').slice(before).pop();
            check('⌥⏎ shows it in Codealong too', !!shown && !!shown.reveal && shown.reveal.key === picked.key, JSON.stringify(shown));
            check('⌥⏎ doesn’t ask this machine to open it', calls('open_from_bar').length === 0, JSON.stringify(sent('open_from_bar')));
          });
        });
    })
    .then(function () {
      if (!picked) return;
      // The mini is connected and in_bar is on, so Waiting on you lists its sessions.
      type(input(), WORD);
      return until(function () { return !!rowOf(WORD); }, 5000)
        .catch(function () {})
        .then(function () {
          key(input(), { key: 'Tab' });
          return wait(300).then(function () {
            var asked = sent('show_waiting').pop();
            check('⇥ answers it in Waiting on you', !!asked && !!asked.context && asked.context.key === picked.key, JSON.stringify(asked));
          });
        });
    })
    .then(function () {
      if (!spoken) {
        if (picked) note('the mini had one session, so nothing was found by what was said there');
        return;
      }
      return clear().then(function () {
        type(input(), SAID);
        return until(function () { return !!rows().filter(function (r) { return r.querySelector('.is-said') && r.textContent.indexOf(SAID) >= 0; })[0]; }, 5000)
          .catch(function () {})
          .then(function () {
            var row = rows().filter(function (r) { return r.textContent.indexOf(SAID) >= 0; })[0];
            check('a session there is found by what was said in it', !!row && !!row.querySelector('.ca-devbadge'), rows().map(function (r) { return r.textContent.slice(0, 60); }).join(' | '));
            check('the machines’ own text search was asked', devicesAsked.indexOf(SAID) >= 0, devicesAsked.join(' | '));
          });
      });
    })
    .then(function () {
      if (!picked) return;
      return clear()
        .then(function () {
          type(input(), WORD);
          return until(function () { return !!rowOf(WORD); }, 5000).catch(function () {});
        })
        .then(function () { return rowOf(WORD) ? menuOf(rowOf(WORD)) : []; })
        .then(function (labels) {
          check('its menu renames it in Waiting on you, which lists the machine’s sessions', labels.indexOf('Rename…') >= 0, labels.join(' | '));
          // Waiting on you stops listing other machines' sessions (`[devices] in_bar` off).
          return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) {
            b.devices = Object.assign({}, b.devices, { in_bar: false });
            window.__emit('board', b);
            return wait(300);
          });
        })
        .then(function () { return rowOf(WORD) ? menuOf(rowOf(WORD)) : []; })
        .then(function (labels) {
          check('with Waiting on you not listing them, its menu has no Rename…', labels.length > 0 && labels.indexOf('Rename…') < 0, labels.join(' | '));
          var waited = sent('show_waiting').length;
          var shownBefore = sent('show_main').length;
          key(input(), { key: 'Tab' });
          return wait(300).then(function () {
            var shown = sent('show_main').slice(shownBefore).pop();
            check('and ⇥ shows it in Codealong instead', sent('show_waiting').length === waited && !!shown && !!shown.reveal && shown.reveal.key === picked.key, JSON.stringify(shown));
          });
        });
    })
    .then(function () {
      if (!picked) return;
      // Archived from its menu, it leaves the search at once, before the machine's next feed says
      // so, as this machine's own sessions do (useDevices.ts `useDeviceLanes`).
      type(input(), WORD);
      return until(function () { return !!rowOf(WORD); }, 5000)
        .catch(function () {})
        .then(function () { return rowOf(WORD) ? menuOf(rowOf(WORD)) : []; })
        .then(function (labels) {
          var label = labels.filter(function (l) { return /^Archive/.test(l); })[0];
          check('its menu archives it', !!label, labels.join(' | '));
          if (!label) return;
          menuItem(label)();
          return until(function () { return !rowOf(WORD); }, 3000)
            .catch(function () {})
            .then(function () { check('archived, it leaves the search at once', !rowOf(WORD), rows().map(function (r) { return r.textContent.slice(0, 40); }).join(' | ')); });
        });
    })
    .then(function () {
      if (!spoken) return;
      // Devices turned off while the panel was put away: the board says so, and the view it read
      // last still says they're on.
      return clear()
        .then(function () {
          type(input(), SAID);
          return until(function () { return !!rowOf(SAID); }, 5000).catch(function () {});
        })
        .then(function () {
          check('before devices are turned off, the machine’s session is found', !!rowOf(SAID), rows().map(function (r) { return r.textContent.slice(0, 40); }).join(' | '));
          return window.__TAURI_INTERNALS__.invoke('get_board');
        })
        .then(function (b) {
          b.devices = Object.assign({}, b.devices, { enabled: false });
          window.__emit('board', b);
          return until(function () { return !rowOf(SAID); }, 3000).catch(function () {});
        })
        .then(function () { check('with devices off, it isn’t', !rowOf(SAID), rows().map(function (r) { return r.textContent.slice(0, 40); }).join(' | ')); });
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
