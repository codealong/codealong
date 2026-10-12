// Checks a session whose last turn was cut off (Session.cutOff: its process ended partway
// through, claude/transcript.rs `turn_step`, codex/terminals.rs `held_rollouts`), in the bar's
// peek: its header says the turn was cut off and offers Carry on, which asks for resume_session
// with nothing to say, so the engine tells it to carry on (start.rs `CARRY_ON`), where its
// account's sessions last went on from here, else a terminal; its menu's Resume in says Carry on
// in. The cut-off turn is put on a real Claude Code session, since a board rarely has one.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/cut-off-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : null;
  };

  // A Claude Code session on this Mac that resumes in a terminal, on an account that can go on
  // somewhere other than a terminal too, so its menu has Resume in.
  var board = get('./board.json');
  var options = get('./start-options.json');
  var places = function (account) {
    var a = ((options && options.accounts) || []).filter(function (x) { return x.name === account && x.harness === 'claude'; })[0];
    return a ? a.places.filter(function (p) { return (a.resumes || []).indexOf(p.place) >= 0; }) : [];
  };
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (picked || l.hidden || l.device || s.archived || s.visitor || s.harness !== 'claude' || !s.resumeCommand) return;
      if (places(s.account).length > 1) picked = s;
    });
  });
  if (!picked) { stopped('no Claude Code session on an account that goes on in more than a terminal'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    if (cmd === 'resume_session') {
      window.__calls.push([cmd, arguments[1]]);
      return Promise.resolve({ message: 'Resumed “x” in Ghostty, told to carry on the turn that was cut off.', opened: false });
    }
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.key !== key) return;
          s.status = 'error';
          s.live = false;
          s.pid = null;
          s.hosted = false;
          s.surface = 'cli';
          s.excerpt = 'running `yarn test`';
          s.cutOff = Date.now() - 60000;
        });
      });
      return b;
    });
  };

  (async function () {
    try {
      var line = await until(function () { return document.querySelector('.ca-sessinsp-head .ca-sessinsp-limit'); }, 20000);
      var words = line.querySelector('.ca-sessinsp-limit-words').textContent;
      check('its header says the turn was cut off', /^Its last turn was cut off partway/.test(words), words);
      // Once the window has read where its account's sessions go on.
      var button = await until(function () { return line.querySelector('button'); }, 5000, 'no Carry on once its account’s places were read');
      check('it offers Carry on', button.textContent === 'Carry on', button.textContent);
      words = line.querySelector('.ca-sessinsp-limit-words').textContent;
      check('and leaves going on to the button', words.indexOf('Resume it') < 0, words);
      var more = document.querySelector('.ca-sessinsp-head [aria-label="More"]');
      if (more) {
        var n = menus();
        more.click();
        await until(function () { return menus() > n; }, 3000);
        var labels = window.__menu().map(function (x) { return x[0]; });
        var under = function (group) { return labels.some(function (l) { return l.indexOf(group + ' › ') === 0; }); };
        check('its menu says where it carries on', under('Carry on in') && !under('Resume in'), labels.join(' | '));
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      } else {
        note('no ⋯ button in the peek to read its menu from');
      }
      if (button) {
        button.click();
        await until(function () { return calls('resume_session').length > 0; }, 3000);
        var asked = calls('resume_session')[0][1];
        var where = places(picked.account).map(function (p) { return p.place; });
        check('the button resumes it with nothing to say, so the engine tells it to carry on', asked.key === key && !asked.prompt, JSON.stringify(asked));
        check('where its account goes on', where.indexOf(asked.resume.place) >= 0, asked.resume.place + ' of ' + where.join(', '));
      }
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
