// Checks completed-turn folding (default), compact tool summaries, detailed, and zen modes,
// plus changing `[appearance] tools` in Settings › Appearance (turnFolds.ts, toolDisplay.ts,
// SessionThread.tsx, SettingsDialog.tsx).
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/turn-folds-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var boardNow = null;
  var cfgNow = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 8 * 60e3;
      var item = function (kind, text, n, extra) {
        return Object.assign({ at: at + n * 15000, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {});
      };
      return Promise.resolve({
        before: null,
        after: 20,
        items: [
          item('you', 'Streamline my workflow', 0),
          item('tool', 'Skill · `codealong`', 1, { id: 't1', input: 'codealong' }),
          item('result', '', 2, { id: 't1', output: 'loaded' }),
          item('tool', 'Bash · `python3 -c "print(1)"`', 3, { id: 't2', input: 'python3 -c "print(1)"' }),
          item('result', '', 4, { id: 't2', output: '1' }),
          item('agent', 'Reading your sessions now. Found a needle in the haystack.', 5),
          item('tool', 'Bash · `git log -n 5`', 6, { id: 't3', input: 'git log -n 5' }),
          item('result', '', 7, { id: 't3', output: 'ok' }),
          item('tool', 'Bash · `sed -n 1,5p README.md`', 8, { id: 't4', input: 'sed -n 1,5p README.md' }),
          item('result', '', 9, { id: 't4', output: 'ok' }),
          item('agent', 'Here are the numbers and three improvements.', 10),
        ],
      });
    }
    if (cmd === 'save_config' && args && args.config) {
      window.__calls.push([cmd, args, options]);
      cfgNow = args.config;
      if (boardNow) {
        boardNow = Object.assign({}, boardNow, { tools: (args.config.appearance && args.config.appearance.tools) || 'folded' });
        setTimeout(function () { window.__emit('board', boardNow); }, 0);
      }
      return Promise.resolve({ config: cfgNow, text: '', path: '/tmp/config.toml', error: null, terminals: [], editors: [], chromeProfiles: [] });
    }
    var p = real(cmd, args, options);
    if (cmd === 'get_config') {
      return p.then(function (v) {
        if (cfgNow) v = Object.assign({}, v, { config: cfgNow });
        else cfgNow = v.config;
        return v;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.archived) {
            picked = { key: s.key, lane: l.id };
          }
          if (picked && s.key === picked.key) {
            s.status = 'idle';
          }
        });
      });
      boardNow = board;
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var panel = function (sel) { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp ' + sel)); };
  var openSettings = function () {
    document.querySelector('button[aria-label="Settings"]').click();
    return until(function () { return document.querySelector('.ca-settings'); }, 3000, 'settings dialog').then(function () {
      var nav = Array.prototype.slice.call(document.querySelectorAll('.ca-settings-nav button')).filter(function (b) { return b.textContent.trim() === 'Appearance'; })[0];
      nav.click();
      return until(function () { return document.querySelector('[aria-label="Tool calls in conversations"]'); }, 3000, 'tool calls setting');
    });
  };
  var pickMode = function (label) {
    var seg = document.querySelector('[aria-label="Tool calls in conversations"]');
    var btn = Array.prototype.slice.call(seg.querySelectorAll('button')).filter(function (b) { return b.textContent.trim() === label; })[0];
    btn.click();
  };
  var closeSettings = function () {
    var close = document.querySelector('.ca-settings [aria-label="Close"]');
    if (close) close.click();
    return until(function () { return !document.querySelector('.ca-settings'); }, 3000, 'settings closed');
  };

  say('turn folds check: waiting for the list');
  until(function () { return row(); }, 20000, 'a session row')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return panel('.ca-thread-turnhead').length === 1; }, 10000, 'a collapsed turn fold by default');
    })
    .then(function () {
      var head = panel('.ca-thread-turnhead')[0];
      check('folded by default into one Worked line', /^Worked for 2m · Used a skill and ran 3 commands$/.test(head.textContent.trim()), head.textContent.trim());
      check('only the final reply shows outside the fold', panel('.ca-thread-agent').length === 1 && /Here are the numbers/.test(panel('.ca-thread-agent')[0].textContent), panel('.ca-thread-agent').length + '');
      check('no exposed tool lines while folded', panel('.ca-thread-tool').length === 0 && panel('.ca-thread-runhead').length === 0);
      head.click();
      return until(function () { return panel('.ca-thread-turnbody').length === 1; }, 3000, 'expanded turn fold');
    })
    .then(function () {
      check('expanding reveals intermediate narration and compact 1-line tool summaries',
        panel('.ca-thread-agent').length === 2 && panel('.ca-thread-runhead.is-compact').length === 2 && panel('.ca-thread-tool').length === 0,
        'agents=' + panel('.ca-thread-agent').length + ' runs=' + panel('.ca-thread-runhead.is-compact').length + ' tools=' + panel('.ca-thread-tool').length);
      // Click a compact tool summary to inspect its calls.
      panel('.ca-thread-runhead.is-compact')[0].click();
      return until(function () { return panel('.ca-thread-tool').length === 2; }, 3000, 'calls inside compact run');
    })
    .then(function () {
      check('clicking a compact summary opens its individual tool calls', panel('.ca-thread-tool').length === 2);
      // Collapse the turn fold again, then test Settings › Appearance modes.
      panel('.ca-thread-turnhead')[0].click();
      return until(function () { return panel('.ca-thread-turnbody').length === 0; }, 3000, 'turn fold collapsed again');
    })
    .then(function () { return openSettings(); })
    .then(function () {
      var seg = document.querySelector('[aria-label="Tool calls in conversations"]');
      var pressed = seg.querySelector('button[aria-checked="true"]');
      check('Settings › Appearance shows Folded selected by default', pressed && pressed.textContent.trim() === 'Folded', pressed ? pressed.textContent : 'none');
      pickMode('Compact');
      return until(function () { return panel('.ca-thread-turnhead').length === 0 && panel('.ca-thread-runhead.is-compact').length === 2; }, 3000, 'compact mode live');
    })
    .then(function () {
      check('Compact shows 1-line tool summaries and intermediate replies without turn folds',
        panel('.ca-thread-agent').length === 2 && panel('.ca-thread-runhead.is-compact').length === 2 && panel('.ca-thread-tool').length === 0);
      pickMode('Detailed');
      return until(function () { return panel('.ca-thread-tool').length === 4; }, 3000, 'detailed mode live');
    })
    .then(function () {
      check('Detailed shows individual tool lines', panel('.ca-thread-tool').length === 4 && panel('.ca-thread-runhead.is-compact').length === 0);
      pickMode('Zen');
      return until(function () { return panel('.ca-thread-tool').length === 0 && panel('.ca-thread-agent').length === 1; }, 3000, 'zen mode live');
    })
    .then(function () {
      check('Zen shows only prompts and final replies with no turn fold header',
        panel('.ca-thread-agent').length === 1 && panel('.ca-thread-turnhead').length === 0 && panel('.ca-thread-tool').length === 0);
      return closeSettings();
    })
    .then(function () {
      // ⌘F still finds words inside hidden turns in Zen mode.
      var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true, bubbles: true, cancelable: true }));
      return until(function () { return document.querySelector('.ca-findbar input'); }, 3000, 'find bar');
    })
    .then(function (input) {
      var setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(input, 'needle in the haystack');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return until(function () { return panel('.ca-thread-turnbody.is-zen').length === 1; }, 3000, 'zen match revealed by ⌘F');
    })
    .then(function () {
      check('⌘F reveals hidden intermediate replies in Zen mode', /needle in the haystack/.test(panel('.ca-thread-turnbody.is-zen')[0].textContent));
    })
    .then(done)
    .catch(stopped);
})();
