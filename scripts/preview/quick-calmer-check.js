// Checks the quick panel offering a calm machine while the one a start goes to is strained
// (machineLoad.ts `calmerMachine`): this machine strained, the Mac mini calm with copies of every
// worktree here. The note says both and names the repo; the machine menu says each machine's load;
// ⇧⌘D switches the start to the mini with the same repo, and the note goes, the mini not being
// strained. Then the mini busy: no note, a busy machine only trading one wait for another. Calm
// again, with a prompt typed, ⇧⌘D then ⏎ starts the session on the mini.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh quick '' scripts/preview/quick-calmer-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PLATFORM=linux PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh quick '' scripts/preview/quick-calmer-check.js
(function () {
  var MINI = '9f8e7d6c5b';
  var strained = { level: 'strained', perCpu: 27, cpus: 10, memoryFree: 40, memoryShort: false };
  var calm = { level: 'calm', perCpu: 0.3, cpus: 8, memoryFree: 60, memoryShort: false };
  var busy = { level: 'busy', perCpu: 2.1, cpus: 8, memoryFree: 60, memoryShort: false };
  var miniLoad = calm;
  // This runs before the app's script: the board says this machine is strained, and the mini has
  // a copy of every worktree here, so whatever repo the panel starts in, the mini has it too.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'get_board') return p.then(function (b) { b.load = strained; return b; });
    if (cmd !== 'devices') return p;
    return Promise.all([p, real('get_board')]).then(function (all) {
      var view = all[0];
      var mini = view.devices.filter(function (d) { return d.short === MINI; })[0];
      if (!mini) return view;
      mini.load = miniLoad;
      mini.online = true;
      mini.remoteControl = true;
      var have = {};
      view.lanes.forEach(function (l) { have[l.id] = true; });
      all[1].lanes.forEach(function (l) {
        if (l.hidden || l.device || have['@' + MINI + ':' + l.id]) return;
        var copy = JSON.parse(JSON.stringify(l));
        copy.id = '@' + MINI + ':' + l.id;
        copy.device = { name: mini.name, short: MINI, color: mini.color, online: true, lastSeen: null };
        copy.sessions = [];
        copy.targets = [];
        copy.defaultTarget = null;
        view.lanes.push(copy);
      });
      return view;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var note = function () { return q('.ca-quick-calmer'); };
  var chip = function (title) { return all('.ca-quick-bar .ca-quick-chip').filter(function (c) { return (c.title || '').indexOf(title) >= 0; })[0]; };
  var machine = function () { var c = chip('Machine'); return c ? c.textContent.trim() : ''; };
  var repo = function () { var c = chip('Repo'); return c ? c.textContent.trim() : ''; };
  var press = function (k, mods) { key(input(), Object.assign({ key: k }, mods)); };
  // ⇧⌘D, Ctrl+Shift+D on Linux (PREVIEW_PLATFORM=linux).
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  var calmerKey = function () { press('D', { code: 'KeyD', metaKey: !linux, ctrlKey: linux, shiftKey: true }); };

  say('quick calmer check: waiting for the panel and the machines');
  until(function () { return !!input() && calls('devices').length > 0 && !!chip('Machine'); }, 20000, 'no machine menu in the panel (run with PREVIEW_DEVICES)')
    .then(function () { return until(function () { return !!note(); }, 5000).catch(function () {}); })
    .then(function () {
      var n = note();
      check('a strained start offers the calm machine', !!n, 'no note; machine chip: ' + machine() + ', repo chip: ' + repo());
      if (!n) return;
      var repoNow = repo();
      check('it says this machine is strained, with its numbers', /is strained: load 27 per CPU of 10, 40% of memory free\./.test(n.textContent), n.textContent);
      check('it names the calm machine and the repo it has', n.textContent.indexOf('Mac mini is calm and has ' + repoNow + '.') >= 0, n.textContent + ' | repo chip: ' + repoNow);
      var button = n.querySelector('.ca-btn');
      var keycaps = button ? all('kbd', button).map(function (k) { return k.textContent; }).join(' ') : '';
      check('its button names the machine and its key', !!button && /Use Mac mini/.test(button.textContent) && keycaps === (linux ? 'Ctrl Shift D' : '⇧ ⌘ D'), button ? button.textContent + ' | ' + keycaps : 'no button');
      // The machine menu says each one's load.
      chip('Machine').click();
      return wait(300)
        .then(function () {
          var rows = all('.ca-picker .ca-picker-opt');
          var loads = all('.ca-picker .ca-load').map(function (t) { return t.textContent.trim() + ' (' + t.title + ')'; });
          check('the machine menu says each machine’s load', loads.some(function (l) { return /^Strained/.test(l); }) && loads.some(function (l) { return /^Calm \(Mac mini is calm/.test(l); }), loads.join(' | ') + ' in ' + rows.length + ' rows');
          press('Escape', {});
          return wait(200);
        })
        .then(function () {
          calmerKey();
          return wait(400);
        })
        .then(function () {
          check('⇧⌘D starts it on the calm machine', /Mac mini/.test(machine()), machine());
          check('in the same repo there', repo() === repoNow, repo() + ' (was ' + repoNow + ')');
          check('and the note goes, the calm machine not being strained', !note(), note() ? note().textContent : '');
          check('nothing was started', calls('start_from_quick').length === 0, JSON.stringify(sent('start_from_quick')));
        });
    })
    .then(function () {
      // Back on this machine, with the mini busy: nothing's offered.
      chip('Machine').click();
      return wait(300).then(function () {
        var here = all('.ca-picker .ca-picker-opt').filter(function (o) { return /This machine/.test(o.textContent); })[0];
        check('the machine menu lists this machine', !!here, all('.ca-picker .ca-picker-opt').map(function (o) { return o.textContent; }).join(' | '));
        if (here) here.click();
        return wait(300);
      });
    })
    .then(function () {
      check('back on this machine, the calm one is offered again', !!note() && !/Mac mini/.test(machine()), machine());
      miniLoad = busy;
      return window.__TAURI_INTERNALS__.invoke('devices').then(function (view) {
        window.__emit('devices', view);
        return wait(400);
      });
    })
    .then(function () {
      check('a busy machine isn’t offered', !note(), note() ? note().textContent : '');
      // Calm again, with a prompt typed: ⇧⌘D, then ⏎ starts it there, whatever row was picked.
      miniLoad = calm;
      return window.__TAURI_INTERNALS__.invoke('devices').then(function (view) {
        window.__emit('devices', view);
        type(input(), 'look into the load offer');
        return wait(400);
      });
    })
    .then(function () {
      var repoNow = repo();
      check('offered again once it’s calm', !!note(), machine());
      calmerKey();
      return wait(400).then(function () {
        check('in the repo it was going to start in', /Mac mini/.test(machine()) && repo() === repoNow, machine() + ', ' + repo() + ' (was ' + repoNow + ')');
        press('Enter', {});
        return until(function () { return calls('start_from_quick').length > 0; }, 3000).catch(function () {});
      }).then(function () {
        var spec = (sent('start_from_quick')[0] || {}).spec;
        check('⏎ then starts it on the calm machine', !!spec && spec.laneId.indexOf('@' + MINI + ':') === 0, JSON.stringify(spec || window.__calls.slice(-4)));
        check('with the prompt typed', !!spec && spec.prompt.indexOf('look into the load offer') >= 0, spec ? spec.prompt : '');
      });
    })
    .then(done, stopped);
})();
