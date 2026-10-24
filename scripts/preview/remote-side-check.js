// Checks a session on another machine's reply box (SessionReply.tsx, slash.ts, useSkills.ts,
// SideStrip.tsx): typing `/` lists /side and the session's skills, asked of its own machine by the
// key that names it there (`session_skills`), and a skill goes as its agent takes it;
// `/side <question>` asks a copy there by that key, sends the session nothing, and its answer
// shows above the box as that machine tells of it, with no Pop out (a window of its own finds its
// session on this machine's board); shown again, it's read again, as that machine may have gone on
// with nothing told of it here; a Close that machine didn't hear leaves it open, above the box with
// why, and what it says after still shows; Close closes it there; Delete on a kept one forgets it
// there.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_SKILLS=scripts/preview/skills-sample.json scripts/preview.sh sessions '' scripts/preview/remote-side-check.js
(function () {
  // This runs before the app's script: a Claude Code or Codex session on the Mac mini, idle and
  // taking messages.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  window.__past = [];
  var picked = null;
  var failClose = false;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'side_close' && failClose) {
      failClose = false;
      window.__calls.push([cmd, args, options]);
      return Promise.reject('Mac mini didn’t answer in time.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'devices') return p;
    return p.then(function (view) {
      (view.lanes || []).forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || l.device.short !== '9f8e7d6c5b' || s.visitor || (s.harness !== 'claude' && s.harness !== 'codex')) return;
          Object.assign(s, { status: 'idle', live: true, stoppable: false, stalledSince: null, asks: false, archived: false, snoozedUntil: undefined, title: 'Far A' });
          picked = { key: s.key, lane: l.id, harness: s.harness };
        });
      });
      return view;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var names = function () { return all('.ca-slash-row').map(function (r) { return q('.ca-slash-name', r).textContent.trim().split(' ')[0]; }); };
  var strip = function () { return document.querySelector('.ca-sessinsp .ca-sidestrip'); };
  var turns = function () { return all('.ca-sessinsp .ca-sidestrip-turn').map(function (t) { return t.textContent; }); };
  var enter = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var headButton = function (label) { return all('.ca-sidestrip-head button').filter(function (b) { return b.getAttribute('aria-label') === label; })[0]; };

  (async function () {
    try {
      await until(row, 20000, 'no Claude Code or Codex session on the Mac mini (run with PREVIEW_DEVICES)');
      await wait(300);
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await until(function () { return title() === 'Far A' && !!box(); }, 5000, 'its panel');
      var key = picked.key;
      check('the session is on the Mac mini', key.indexOf('@9f8e7d6c5b:') === 0, key);

      type(box(), '/');
      await until(function () { return names().indexOf('/ship') >= 0; }, 5000, 'no skills for /: ' + names().join(' '));
      var asked = calls('session_skills').map(function (c) { return c[1].key; });
      check('its skills are asked of its own machine, by the key that names it', asked.length >= 1 && asked.every(function (k) { return k === key; }), asked.join(', '));
      check('none are listed from this machine’s folders', calls('skills').length === 0, JSON.stringify(calls('skills').map(function (c) { return c[1]; })));
      var got = names();
      check('/side is offered, with Codealong’s own commands first', got[0] === '/fork' && got.indexOf('/side') === 1, got.join(' '));
      check('only the skills its agent reads', picked.harness === 'codex' ? got.indexOf('/probe') >= 0 && got.indexOf('/shape-up') < 0 : got.indexOf('/shape-up') >= 0 && got.indexOf('/probe') < 0, got.join(' '));

      var sends = calls('send_to_session').length;
      type(box(), '/ship the fix');
      await wait(50);
      enter();
      await until(function () { return calls('send_to_session').length > sends; }, 5000, 'the skill wasn’t sent');
      var sent = calls('send_to_session').slice(-1)[0][1];
      check('a skill goes to it as its agent takes it', sent.key === key && sent.text === (picked.harness === 'codex' ? '$ship the fix' : '/ship the fix'), JSON.stringify(sent));
      sends = calls('send_to_session').length;

      type(box(), '/side why a queue?');
      await wait(50);
      enter();
      await until(strip, 5000, 'no strip after /side');
      var asks = calls('side_ask');
      check('/side asks a copy on its machine, by the key that names it', asks.length === 1 && asks[0][1].key === key && asks[0][1].text === 'why a queue?', JSON.stringify(asks.map(function (c) { return c[1]; })));
      check('and sends the session nothing', calls('send_to_session').length === sends);
      check('its side conversation is read from there too', calls('side_get').some(function (c) { return c[1].key === key; }) && calls('side_past').some(function (c) { return c[1].key === key; }));

      // What that machine tells of it, by the key the session has here (peers.rs `Msg::Side`).
      var side = Object.assign({}, window.__sides[key], { turns: [{ question: 'why a queue?', answer: 'Because sends must keep their order.', done: true, at: Date.now() }], running: false });
      window.__sides[key] = side;
      window.__emit('side', side);
      await until(function () { return turns().length === 1 && turns()[0].indexOf('Because sends must keep their order.') > 0; }, 3000, 'the answer didn’t show: ' + turns().join(' / '));
      check('its answer shows above the box', true);
      check('with no Pop out', !headButton('Pop out') && !!headButton('Close'), all('.ca-sidestrip-head button').map(function (b) { return b.getAttribute('aria-label'); }).join(' '));

      // It goes on there while nothing reaches this machine; shown again, it's read again.
      window.__sides[key] = Object.assign({}, side, { turns: side.turns.concat([{ question: 'and without one?', answer: 'Then they race.', done: true, at: Date.now() }]) });
      var other = all('[id^="row-"]').filter(function (r) { return r !== row(); })[0];
      other.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await until(function () { return title() !== 'Far A'; }, 3000, 'another session’s panel');
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await until(function () { return turns().length === 2 && turns()[1].indexOf('Then they race.') > 0; }, 3000, 'not read again: ' + turns().join(' / '));
      check('shown again, it’s as that machine has it now', true);
      side = window.__sides[key];

      // A Close that machine didn't hear.
      failClose = true;
      headButton('Close').click();
      await until(function () { return calls('side_close').length === 1 && !!strip() && !!q('.ca-sidestrip-error', strip()); }, 5000, 'not back after a Close that failed');
      check('a Close that machine didn’t hear leaves it above the box, with why', q('.ca-sidestrip-error', strip()).textContent === 'Mac mini didn’t answer in time.' && turns().length === 2, q('.ca-sidestrip-error', strip()).textContent);
      side = Object.assign({}, side, { turns: side.turns.concat([{ question: 'and now?', answer: 'Still here.', done: true, at: Date.now() }]) });
      window.__sides[key] = side;
      window.__emit('side', side);
      await until(function () { return turns().length === 3; }, 3000, 'what it said after didn’t show: ' + turns().join(' / '));
      check('and what it says after still shows', turns()[2].indexOf('Still here.') > 0, turns().join(' / '));

      headButton('Close').click();
      await until(function () { return !strip(); }, 5000, 'Close left it');
      check('Close closes it on its machine', calls('side_close').length === 2 && calls('side_close')[1][1].key === key && !q('.ca-sidestrip-error'), JSON.stringify(calls('side_close').map(function (c) { return c[1]; })));

      type(box(), '/side');
      await wait(50);
      enter();
      await until(function () { return all('.ca-sessinsp .ca-sidestrip-kept').length === 1; }, 5000, 'no kept one listed');
      all('.ca-sessinsp .ca-sidestrip-kept')[0].click();
      await until(function () { return !!headButton('Delete'); }, 3000, 'no Delete');
      headButton('Delete').click();
      await until(function () { return calls('side_forget').length === 1; }, 3000, 'Delete forgot nothing');
      var forgot = calls('side_forget')[0][1];
      check('Delete forgets it on its machine', forgot.key === key && forgot.id === side.id, JSON.stringify(forgot));
      done();
    } catch (e) {
      stopped(String(e && e.message || e));
    }
  })();
})();
